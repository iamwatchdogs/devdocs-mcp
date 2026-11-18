//! Text normalization helpers over fetched page bodies.
//!
//! [`strip_ansi`] removes terminal escape sequences and control characters
//! from a page body before any downstream stage hashes, counts, or
//! segments it (docs/implementation/07 §Normalization pipeline, the ANSI
//! pre-stage). A hand-rolled byte scan — no dependency.

/// Strips ANSI escape sequences and bare control characters from
/// `text`, keeping `\n`, `\r`, and `\t`.
///
/// Removed shapes:
///
/// - CSI sequences: `ESC [` through the final byte in `0x40..=0x7E`,
///   consuming the parameter bytes (`0x30..=0x3F`) and intermediate bytes
///   (`0x20..=0x2F`) in between. An unterminated CSI sequence runs to the
///   end of the input.
/// - String-mode sequences: `ESC ]` (OSC) through a BEL (`0x07`) or an
///   ST (`ESC \`, or the two-byte UTF-8 encoding of the C1 byte
///   `U+009C`); `ESC P` (DCS), `ESC X` (SOS), `ESC ^` (PM), and
///   `ESC _` (APC) through an ST — xterm terminates those with ST only,
///   so a BEL inside their payload is payload, not a terminator (a BEL
///   terminator there under-strips, leaving payload tail live). An
///   unterminated string-mode sequence runs to the end of the input.
/// - Every other ECMA-48 escape: `ESC` plus its intermediate bytes
///   (`0x20..=0x2F`) and final byte (`0x30..=0x7E`) — cursor saves,
///   charset designates, and their kin. An unterminated one runs to the
///   end of the input.
/// - Every other C0 control byte (including a lone `ESC`), except the
///   kept `\n`, `\r`, `\t`.
/// - Every C1 control character (`U+0080`–`U+009F`): their UTF-8
///   encodings pass a byte-level C0 filter, and `U+009B` is the 8-bit CSI
///   introducer terminals honor, so a planted `\u{9b}31m` would repaint
///   the terminal. Recall-first: nothing in the range is kept.
///
/// The pass is byte-level: CSI and OSC framing bytes are ASCII, so a
/// byte scan cannot split a multi-byte UTF-8 character (its bytes are all
/// non-ASCII and never match a control or escape byte). The one
/// multi-byte shape the scan must handle itself is the C1 range, whose
/// two-byte UTF-8 encodings (`0xC2 0x80..=0x9F`) the run detector stops
/// at explicitly.
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
            // The C1 control range `U+0080`–`U+009F` encodes as
            // `0xC2 0x80..=0x9F`; the two bytes drop together. A `0xC2`
            // lead byte followed by a print-level continuation
            // (`U+00A0`–`U+00BF`: NBSP, copyright, and their kin) falls
            // through to the plain-run arm and survives.
            0xc2 if bytes
                .get(i + 1)
                .is_some_and(|&b| (0x80..=0x9f).contains(&b)) =>
            {
                i += 2;
            }
            _ => {
                // Collect the run of plain bytes up to the next
                // control/escape byte (or a C1 encoding), then push it as
                // UTF-8. The run's end always sits on a char boundary:
                // control bytes never appear inside a multi-byte
                // sequence, and a C1 lead byte ends the run whole. The
                // scan starts one byte in, so the run always advances —
                // a 0xC2 lead byte whose continuation is not a C1 control
                // (a print character such as NBSP or ©) must not stall on
                // itself.
                let end = bytes[i + 1..]
                    .iter()
                    .position(|&b| b == 0x1b || b < 0x20 || b == 0x7f || b == 0xc2)
                    .map_or(bytes.len(), |at| i + 1 + at);
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
        // OSC (`ESC ]`): a payload terminated by BEL (`0x07`) or ST
        // (`ESC \` or the two-byte UTF-8 encoding of the C1 byte
        // U+009C).
        b']' => {
            let mut i = start + 2;
            while i < bytes.len() {
                match bytes[i] {
                    0x07 => return i + 1,
                    0x1b if bytes.get(i + 1) == Some(&b'\\') => return i + 2,
                    0xc2 if bytes.get(i + 1) == Some(&0x9c) => return i + 2,
                    _ => i += 1,
                }
            }
            bytes.len()
        }
        // The other string-mode sequences — DCS (`ESC P`), SOS
        // (`ESC X`), PM (`ESC ^`), APC (`ESC _`): xterm terminates them
        // with ST only (`ESC \` or the two-byte UTF-8 encoding of
        // U+009C), so a BEL inside the payload is payload and never ends
        // them. A lone `0x9C` byte is a continuation byte of legitimate
        // text and never terminates — only the `0xC2 0x9C` pair is the
        // C1 ST. Unterminated, the rest of the input drops with the
        // payload — the documented over-strip direction.
        b'P' | b'X' | b'^' | b'_' => {
            let mut i = start + 2;
            while i < bytes.len() {
                if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
                    return i + 2;
                }
                if bytes[i] == 0xc2 && bytes.get(i + 1) == Some(&0x9c) {
                    return i + 2;
                }
                i += 1;
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
    fn string_mode_payloads_are_removed() {
        // DCS (`ESC P … ST`): the payload drops whole, not just the
        // two-byte escape head.
        assert_eq!(strip_ansi("before\x1bPpayload\x1b\\after"), "beforeafter");
        // SOS, PM, APC.
        assert_eq!(strip_ansi("a\x1bXsos\x1b\\b"), "ab");
        assert_eq!(strip_ansi("a\x1b^pm\x1b\\b"), "ab");
        assert_eq!(strip_ansi("a\x1b_apc\x1b\\b"), "ab");
    }

    #[test]
    fn a_bel_inside_a_non_osc_payload_is_payload_not_a_terminator() {
        // xterm terminates DCS/SOS/PM/APC with ST only: the BEL inside
        // the payload must not end it, or the payload tail after the BEL
        // survives live (an under-strip).
        assert_eq!(
            strip_ansi("before\x1bPfirst\x07secret\x1b\\after"),
            "beforeafter"
        );
        // Without an ST the payload is unterminated and runs to the end
        // of the input — the documented over-strip direction.
        assert_eq!(strip_ansi("before\x1bPpayload\x07after"), "before");
    }

    #[test]
    fn string_mode_payloads_terminate_on_the_c1_st_byte() {
        // ST is also the C1 byte U+009C (xterm ctlseqs): the two-byte
        // UTF-8 encoding ends the payload like `ESC \` does. Input is
        // valid UTF-8, so only the two-byte form can appear — a lone
        // 0x9C byte is a continuation byte of legitimate text and never
        // terminates.
        assert_eq!(strip_ansi("\x1bPpayload\u{9c}tail"), "tail");
        assert_eq!(strip_ansi("\x1b]0;title\u{9c}rest"), "rest");
        // A continuation byte 0x9C without the 0xC2 lead (inside CJK
        // text) is not an ST: the payload keeps running to its real
        // terminator.
        assert_eq!(strip_ansi("a\x1bP亜\u{9c}\x1b\\b"), "ab");
    }

    #[test]
    fn an_unterminated_string_mode_payload_runs_to_the_end() {
        assert_eq!(strip_ansi("keep\x1bPpayload"), "keep");
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
    fn c1_control_characters_are_removed() {
        // U+009B is the 8-bit CSI introducer terminals honor: a planted
        // `\u{9b}31m` must not keep its introducer, or the terminal
        // repaints on the trailing text. The introducer drops; the
        // remaining bytes are inert plain text.
        let stripped = strip_ansi("\u{9b}31mred\u{9b}0m");
        assert_eq!(stripped, "31mred0m");
        assert!(!stripped.contains('\u{9b}'));
        // The whole C1 range drops, two-byte UTF-8 encodings included.
        for code in [0x80u32, 0x84, 0x9b, 0x9f] {
            let ch = char::from_u32(code).unwrap();
            let text = format!("a{ch}b");
            assert_eq!(strip_ansi(&text), "ab", "C1 U+{code:04X} must drop");
        }
        // A C1 introducer mid-run ends the run: the bytes around it stay.
        assert_eq!(strip_ansi("keep\u{9b}[31mme"), "keep[31mme");
    }

    #[test]
    fn print_level_c2_continuations_survive() {
        // `0xC2` lead bytes above the C1 range are print characters:
        // NBSP, inverted question mark, and their kin.
        assert_eq!(strip_ansi("a\u{a0}b\u{bf}c"), "a\u{a0}b\u{bf}c");
        // A lone 0xC2 byte cannot occur in valid UTF-8, but a `0xC2`
        // lead followed by a non-C1 continuation is not a C1 control.
        assert_eq!(strip_ansi("caf\u{a9}-au lait"), "caf\u{a9}-au lait");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(strip_ansi(""), "");
    }
}
