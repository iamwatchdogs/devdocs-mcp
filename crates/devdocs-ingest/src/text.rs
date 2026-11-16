//! Text normalization helpers over fetched page bodies.
//!
//! [`strip_ansi`] removes terminal escape sequences and control characters
//! from a page body before any downstream stage hashes, counts, or
//! segments it (docs/implementation/07 §Normalization pipeline, the ANSI
//! pre-stage). A hand-rolled byte scan — no dependency.

/// Strips ANSI escape sequences and bare C0 control characters from
/// `text`, keeping `\n`, `\r`, and `\t`.
///
/// Removed shapes:
///
/// - CSI sequences: `ESC [` through the final byte in `0x40..=0x7E`,
///   consuming the parameter bytes (`0x30..=0x3F`) and intermediate bytes
///   (`0x20..=0x2F`) in between. An unterminated CSI sequence runs to the
///   end of the input.
/// - OSC sequences: `ESC ]` through a BEL (`0x07`) or an ST (`ESC \`).
///   An unterminated OSC sequence runs to the end of the input.
/// - Every other ECMA-48 escape: `ESC` plus its intermediate bytes
///   (`0x20..=0x2F`) and final byte (`0x30..=0x7E`) — cursor saves,
///   charset designates, and their kin. An unterminated one runs to the
///   end of the input.
/// - Every other C0 control byte (including a lone `ESC`), except the
///   kept `\n`, `\r`, `\t`.
///
/// The pass is byte-level: CSI and OSC framing bytes are ASCII, so a
/// byte scan cannot split a multi-byte UTF-8 character (its bytes are all
/// non-ASCII and never match a control or escape byte).
#[must_use]
pub fn strip_ansi(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            // ESC: the escape sequences and the bare control byte.
            0x1b => {
                i = skip_escape(bytes, i);
            }
            // The kept whitespace controls.
            b'\n' | b'\r' | b'\t' => {
                out.push(bytes[i] as char);
                i += 1;
            }
            // Every other C0 control plus DEL.
            b if b < 0x20 || b == 0x7f => {
                i += 1;
            }
            _ => {
                // Collect the run of plain bytes up to the next
                // control/escape byte, then push it as UTF-8. The run's
                // end always sits on a char boundary: control bytes never
                // appear inside a multi-byte sequence.
                let end = bytes[i..]
                    .iter()
                    .position(|&b| b == 0x1b || b < 0x20 || b == 0x7f)
                    .map_or(bytes.len(), |at| i + at);
                // Safety of `from_utf8` avoided: push the slice through
                // `str` — the slice is the original UTF-8 text between
                // removed bytes, so it is valid.
                out.push_str(&text[i..end]);
                i = end;
            }
        }
    }
    out
}

/// Returns the index just past the escape sequence starting at `start`
/// (which holds `ESC`), or `bytes.len()` when the sequence is
/// unterminated.
fn skip_escape(bytes: &[u8], start: usize) -> usize {
    let Some(&next) = bytes.get(start + 1) else {
        return start + 1;
    };
    match next {
        // CSI: `ESC [` params intermediates final-byte.
        b'[' => {
            let mut i = start + 2;
            while i < bytes.len() && (0x20..=0x3f).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() && (0x40..=0x7e).contains(&bytes[i]) {
                i + 1
            } else {
                bytes.len()
            }
        }
        // OSC: `ESC ]` payload terminated by BEL or ST (`ESC \`).
        b']' => {
            let mut i = start + 2;
            while i < bytes.len() {
                match bytes[i] {
                    0x07 => return i + 1,
                    0x1b if bytes.get(i + 1) == Some(&b'\\') => return i + 2,
                    _ => i += 1,
                }
            }
            bytes.len()
        }
        // Any other escape: `ESC` plus intermediate bytes (`0x20..=0x2F`)
        // and one final byte (`0x30..=0x7E`) — a charset designate such
        // as `ESC ( B`, a cursor save such as `ESC 7`. Unterminated, the
        // rest of the input drops with it.
        _ => {
            let mut i = start + 1;
            while i < bytes.len() && (0x20..=0x2f).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() && (0x30..=0x7e).contains(&bytes[i]) {
                i + 1
            } else {
                bytes.len()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(
            strip_ansi("# Title\n\nbody text\r\n\ttab"),
            "# Title\n\nbody text\r\n\ttab"
        );
    }

    #[test]
    fn csi_sequences_are_removed() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        // Parameter and intermediate bytes are consumed with the sequence.
        assert_eq!(strip_ansi("\x1b[1;4;38;5;196mhot\x1b[m"), "hot");
        assert_eq!(strip_ansi("\x1b[?25lblink\x1b[?25h"), "blink");
        // Cursor movement with a space intermediate byte.
        assert_eq!(strip_ansi("a\x1b[2 Bb"), "ab");
    }

    #[test]
    fn an_unterminated_csi_sequence_runs_to_the_end() {
        assert_eq!(strip_ansi("keep\x1b[31"), "keep");
        assert_eq!(strip_ansi("keep\x1b["), "keep");
    }

    #[test]
    fn osc_sequences_are_removed() {
        // BEL-terminated.
        assert_eq!(strip_ansi("\x1b]0;window title\x07rest"), "rest");
        // ST-terminated (`ESC \`).
        assert_eq!(strip_ansi("\x1b]8;;https://x\x1b\\link"), "link");
    }

    #[test]
    fn an_unterminated_osc_sequence_runs_to_the_end() {
        assert_eq!(strip_ansi("keep\x1b]0;trailing"), "keep");
    }

    #[test]
    fn bare_c0_controls_are_removed_but_line_breaks_survive() {
        assert_eq!(strip_ansi("a\u{7}b\u{8}c"), "abc");
        assert_eq!(strip_ansi("a\u{1}b"), "ab");
        assert_eq!(strip_ansi("a\u{7f}b"), "ab");
        assert_eq!(
            strip_ansi("line\nkeep\rreturn\ttab"),
            "line\nkeep\rreturn\ttab"
        );
    }

    #[test]
    fn a_lone_escape_byte_is_removed() {
        assert_eq!(strip_ansi("trailing\u{1b}"), "trailing");
        assert_eq!(strip_ansi("keep\u{1b}"), "keep");
    }

    #[test]
    fn other_escapes_are_removed_whole() {
        // `ESC ( B` names the ASCII charset: intermediates and the final
        // byte drop with the escape.
        assert_eq!(strip_ansi("\u{1b}(Btext"), "text");
        // A single-character escape such as `ESC 7` (save cursor).
        assert_eq!(strip_ansi("a\u{1b}7b"), "ab");
        // Unterminated intermediates run to the end.
        assert_eq!(strip_ansi("keep\u{1b}("), "keep");
    }

    #[test]
    fn multibyte_characters_survive() {
        assert_eq!(strip_ansi("héllo ✅ wörld"), "héllo ✅ wörld");
        assert_eq!(strip_ansi("\x1b[32m✔ done\x1b[0m"), "✔ done");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(strip_ansi(""), "");
    }
}
