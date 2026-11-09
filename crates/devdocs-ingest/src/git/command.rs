//! The git command builder and runner: every invocation is one argv
//! vector against the system binary with a hardened child env
//! (docs/implementation/09 §Git subprocess safety).

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::IngestError;

/// The `GIT_*` environment variables every invocation strips, so an
/// ambient value cannot redirect the child to another repository or
/// object store, inject config, hijack prompting or SSH, or disable the
/// HTTPS certificate check (docs/implementation/09 §Git subprocess
/// safety). Removing
/// `GIT_CONFIG_COUNT` disables the whole ambient
/// `GIT_CONFIG_KEY_i`/`GIT_CONFIG_VALUE_i` config family, and removing
/// `GIT_CONFIG_PARAMETERS` disables the legacy parameter-string channel
/// git still honors alongside it — both carry config that a hostile
/// repo's `.gitattributes` could turn into a smudge-filter execution.
const STRIPPED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_ASKPASS",
    "GIT_SSH_COMMAND",
    "GIT_SSH_VARIANT",
    "GIT_SSL_NO_VERIFY",
    "GIT_STDERR",
];

/// Per-command git config, applied to every invocation: checkout hooks
/// and LFS filters cannot run, and the local `file://` transport stays
/// enabled against a host config that disables it.
const BASE_CONFIG: &[&str] = &[
    "core.hooksPath=/dev/null",
    "filter.lfs.process=",
    "filter.lfs.smudge=",
    "filter.lfs.clean=",
    "protocol.file.allow=user",
];

/// The [`run_git`] wait-loop poll interval.
const WAIT_POLL: Duration = Duration::from_millis(100);

/// The grace a pipe reader gets after the deadline: the direct git child
/// is dead by then, so a normal exit delivers its buffer within this
/// window, while a stalled transport helper never closes the pipe and
/// expires instead (a leaked reader thread is bounded by the process
/// lifetime).
const READER_GRACE: Duration = Duration::from_secs(2);

/// Set by the CLI before it exits on the total-cap path: the pipeline's
/// `timeout` drops the waiting future but the `spawn_blocking` closure
/// keeps running with its own per-invocation deadline, which can outlive
/// the total cap — the flag makes [`wait_until`] stop the in-flight git
/// child so the clone-dir cleanup can proceed and the runtime shutdown
/// grace stays short.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Requests that every running [`run_child`] wait loop kill and reap its
/// child at the next poll. The CLI sets this before exiting on the
/// total-cap path so the in-flight git child is killed and the clone dir
/// cleanup can proceed (docs/implementation/09 §Git subprocess safety).
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Builds the command one `git` invocation runs: `git`, the per-command
/// `-c` config (base plus extras), then `args`, from `cwd`. The child env
/// is hardened here: the prompt, system config, and locale pins, then the
/// removal of every [`STRIPPED_ENV`] variable so an ambient value cannot
/// reach the git child (docs/implementation/09 §Git subprocess safety).
/// Extracted so tests assert on the exact command `run_git` executes,
/// not on a seam beside it.
fn build_git_command(cwd: &Path, args: &[&str], extra_config: &[&str]) -> Command {
    let mut command = Command::new("git");
    for config in BASE_CONFIG
        .iter()
        .copied()
        .chain(extra_config.iter().copied())
    {
        command.arg("-c").arg(config);
    }
    command.args(args).current_dir(cwd);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_CONFIG_NOSYSTEM", "1");
    // Pin the locale: error classification reads git's stderr text (the
    // missing-tag and SHA-refusal paths), and a translated diagnostic
    // would break the match. `LANGUAGE` overrides `LC_ALL` for gettext,
    // so it must go too.
    command.env("LC_ALL", "C");
    command.env_remove("LANGUAGE");
    // Isolate the user's global git config too: a hostile repo's
    // `.gitattributes` could otherwise trigger an operator-defined global
    // smudge filter via `core.attributesFile` during checkout. Git's
    // compiled-in default credential helper does not come from
    // ~/.gitconfig, and the per-command `-c` overrides still win.
    #[cfg(windows)]
    command.env("GIT_CONFIG_GLOBAL", "NUL");
    #[cfg(not(windows))]
    command.env("GIT_CONFIG_GLOBAL", "/dev/null");
    for key in STRIPPED_ENV {
        command.env_remove(key);
    }
    // After the strip: a noninteractive ssh transport. `SSH_ASKPASS` and
    // `DISPLAY` survive the env removal and the ssh child still holds the
    // controlling tty, so a passphrase-protected `ssh://` source would
    // wait for human input until the ingest cap; BatchMode makes ssh fail
    // promptly on any passphrase or host-key prompt while agent-backed
    // keys keep working (docs/implementation/09 §Git subprocess safety).
    command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    command
}

/// Runs one `git` invocation from `cwd` and returns trimmed stdout.
///
/// The command is an argv vector against the system binary — no shell, no
/// interpolated command line — built by [`build_git_command`]
/// (docs/implementation/09 §Git subprocess safety). The child is waited on
/// with a bounded loop up to the total ingest cap: at expiry it is killed
/// and reaped, so a deadline drop cannot orphan the clone's network I/O
/// (docs/implementation/09 §Git subprocess safety).
pub(crate) fn run_git(
    cwd: &Path,
    args: &[&str],
    extra_config: &[&str],
) -> Result<String, IngestError> {
    let mut command = build_git_command(cwd, args, extra_config);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    // The child stays in the CLI's process group: terminal Ctrl-C reaches
    // git and its transport helpers directly (they share git's group), so
    // the ctrlc handler's cleanup cannot leave a slow clone running after
    // the CLI exits.
    let mut child = command.spawn().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => IngestError::GitBinaryMissing,
        _ => IngestError::GitSpawn {
            message: error.to_string(),
        },
    })?;
    run_child(
        &mut child,
        format!("git {}", args.join(" ")),
        Instant::now() + crate::pipeline::TOTAL_INGEST_CAP,
    )
}

/// Waits on `child` up to `deadline`, reads its piped output, and maps the
/// outcome onto the ingest errors. Extracted so tests drive it with an
/// arbitrary child.
fn run_child(
    child: &mut Child,
    command_text: String,
    deadline: Instant,
) -> Result<String, IngestError> {
    // Read the pipes on threads while the wait loop polls: a child whose
    // output fills the pipe buffer must not block on a write while it is
    // never read. The joins are bounded (see [`wait_for_reader`]): a
    // stalled transport helper can hold the pipe write end open even
    // after the direct child is dead, so an unbounded join would hang
    // past the deadline.
    let (stdout_reader, stdout_receiver) = read_pipe(child.stdout.take());
    let (stderr_reader, stderr_receiver) = read_pipe(child.stderr.take());
    match wait_until(child, deadline) {
        Ok(Some(status)) => {
            let stdout = wait_for_reader(stdout_reader, &stdout_receiver);
            let stderr = wait_for_reader(stderr_reader, &stderr_receiver);
            if !status.success() {
                return Err(IngestError::GitCommand {
                    command: command_text,
                    stderr: stderr_tail(&stderr),
                    // The untruncated text, kept for the adapter's refusal
                    // classification: a refusal line early in the stream
                    // can fall out of the 512-byte display tail.
                    full_stderr: String::from_utf8_lossy(&stderr).into_owned(),
                });
            }
            Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
        }
        Ok(None) => {
            kill_and_reap(child);
            wait_for_reader(stdout_reader, &stdout_receiver);
            wait_for_reader(stderr_reader, &stderr_receiver);
            Err(IngestError::GitTimeout {
                command: command_text,
                seconds: crate::pipeline::TOTAL_INGEST_CAP.as_secs(),
            })
        }
        Err(error) => {
            kill_and_reap(child);
            wait_for_reader(stdout_reader, &stdout_receiver);
            wait_for_reader(stderr_reader, &stderr_receiver);
            Err(IngestError::GitSpawn {
                message: error.to_string(),
            })
        }
    }
}

/// Waits until the child exits or the deadline passes: `Ok(Some(status))`
/// on exit, `Ok(None)` at expiry or after [`request_shutdown`] — either
/// way the caller kills and reaps. A shutdown expiry surfaces as the
/// deadline-expiry `GitTimeout` outcome: the flag is only set on the
/// total-cap path, so the run already ended in a timeout.
fn wait_until(
    child: &mut Child,
    deadline: Instant,
) -> io::Result<Option<std::process::ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline || SHUTDOWN.load(Ordering::SeqCst) {
            return Ok(None);
        }
        std::thread::sleep(WAIT_POLL);
    }
}

/// Kills the direct child and reaps it so no git process outlives the
/// deadline. git's transport helpers observe the closed pipe and exit on
/// their next write, but a stalled helper never writes again and holds
/// the pipes open — the reader joins are bounded, so the deadline path
/// always returns `GIT_TIMEOUT` (docs/implementation/09 §Git subprocess
/// safety).
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads one pipe to end on a worker thread and hands the buffer back
/// through a channel, so the caller can wait a bounded time instead of
/// joining unconditionally. The handle exists only to be dropped — it
/// detaches the reader when the wait expires.
fn read_pipe<R: io::Read + Send + 'static>(
    pipe: Option<R>,
) -> (
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<Vec<u8>>,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buffer);
        }
        let _ = sender.send(buffer);
    });
    (reader, receiver)
}

/// Waits a short grace for a pipe reader's buffer; a panicked reader
/// yields an empty buffer too. On expiry the buffer is dropped, the
/// receiver is released with the caller's scope, and the detached reader
/// thread ends when the process does — the caller has already killed the
/// direct child.
fn wait_for_reader(
    reader: std::thread::JoinHandle<()>,
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
) -> Vec<u8> {
    drop(reader);
    receiver.recv_timeout(READER_GRACE).unwrap_or_default()
}

/// The bounded stderr tail for an error: at most the last 512 bytes of
/// the trimmed, lossy-decoded text, cut on a char boundary, so it can be
/// shorter than 512 bytes. No full dump.
fn stderr_tail(stderr: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(stderr);
    let text = lossy.trim();
    let mut start = text.len().saturating_sub(512);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::stderr_tail;
    use crate::error::IngestError;

    #[cfg(unix)]
    use std::os::unix::process::CommandExt;

    /// A stderr tail cut mid-character advances to the next char boundary:
    /// a three-byte payload whose 512-byte back-cut lands inside a
    /// character yields a whole-character tail, never a panic.
    #[test]
    fn the_stderr_tail_cuts_on_a_char_boundary() {
        // 1800 bytes; the cut lands at byte 1288, the second byte of
        // character 429 — the loop advances to the boundary at 1291.
        let long = "€".repeat(600);
        let tail = stderr_tail(long.as_bytes());
        assert!(std::str::from_utf8(tail.as_bytes()).is_ok());
        assert!(tail.chars().all(|c| c == '€'));
        assert_eq!(tail.chars().count(), 170);

        // Below the cap the whole text comes back.
        assert_eq!(stderr_tail(b"short".as_slice()), "short");
    }

    /// The builder `run_git` executes hardens the child env: every
    /// ambient `GIT_*` control and location variable is removed, the
    /// prompt, system, and user-global config pins are set, and the ssh
    /// transport is pinned noninteractive — what an operator shell can
    /// carry into the process never reaches the git child
    /// (docs/implementation/09 §Git subprocess safety).
    #[test]
    fn the_git_builder_strips_the_ambient_git_env_from_the_child() {
        let command = super::build_git_command(Path::new("."), &["status"], &[]);

        let envs: std::collections::HashMap<_, _> = command.get_envs().collect();
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_ASKPASS",
            "GIT_SSH_VARIANT",
            "GIT_SSL_NO_VERIFY",
            "GIT_STDERR",
            "LANGUAGE",
        ] {
            assert_eq!(
                envs.get(std::ffi::OsStr::new(key)),
                Some(&None),
                "{key} must be removed from the child env"
            );
        }
        for key in [
            "GIT_TERMINAL_PROMPT",
            "GIT_CONFIG_NOSYSTEM",
            "GIT_CONFIG_GLOBAL",
            "GIT_SSH_COMMAND",
            "LC_ALL",
        ] {
            assert!(
                envs.get(std::ffi::OsStr::new(key))
                    .is_some_and(Option::is_some),
                "{key} must be pinned on the child env"
            );
        }
    }

    /// A child that exits before the deadline yields its status through
    /// `run_child` (unix-gated: the hanging fixture is `sleep`).
    #[cfg(unix)]
    #[test]
    fn run_child_collects_a_child_that_finishes_before_the_deadline() {
        // A process-global shutdown set by a concurrent test would kill
        // this short-lived child; hold the shared test lock instead.
        let _lock = crate::git::test_util::TEMP_CLONE_ORDER.lock().unwrap();
        let mut child = Command::new("echo")
            .arg("done")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = super::run_child(
            &mut child,
            "git status".to_owned(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(stdout, "done");
    }

    /// A child that outlives the deadline surfaces the `GIT_TIMEOUT`
    /// error and is killed and reaped — no orphan process
    /// (docs/implementation/09 §Git subprocess safety). Unix-gated: the
    /// hanging fixture is `sleep`.
    #[cfg(unix)]
    #[test]
    fn run_child_kills_a_child_that_outlives_the_deadline() {
        let mut command = Command::new("sleep");
        command
            .arg("30")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().unwrap();

        let error = super::run_child(
            &mut child,
            "git clone slow".to_owned(),
            Instant::now() + Duration::from_millis(100),
        )
        .unwrap_err();

        match &error {
            IngestError::GitTimeout { command, seconds } => {
                assert_eq!(command, "git clone slow");
                assert_eq!(*seconds, crate::pipeline::TOTAL_INGEST_CAP.as_secs());
                assert!(error.to_string().starts_with("GIT_TIMEOUT:"));
            }
            other => panic!("expected GitTimeout, got {other:?}"),
        }
        // The kill and reap happened before the error returned.
        assert!(child.try_wait().unwrap().is_some());
    }

    /// A child that outlives the deadline while grandchildren keep the
    /// pipes open — like a stalled git transport helper — must still
    /// surface `GIT_TIMEOUT` promptly: the reader joins are bounded, so
    /// the deadline path never blocks on a pipe that never ends
    /// (unix-gated: the fixture is `sh` + `sleep`).
    #[cfg(unix)]
    #[test]
    fn run_child_times_out_when_grandchildren_hold_the_pipes_open() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & sleep 30")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        let start = Instant::now();
        let error = super::run_child(
            &mut child,
            "git clone slow".to_owned(),
            Instant::now() + Duration::from_millis(100),
        )
        .unwrap_err();
        let elapsed = start.elapsed();

        match &error {
            IngestError::GitTimeout { command, .. } => {
                assert_eq!(command, "git clone slow");
            }
            other => panic!("expected GitTimeout, got {other:?}"),
        }
        // Bounded: the deadline plus the reader grace, far below the
        // 30-second sleep fixtures an unbounded join would wait on.
        assert!(elapsed < Duration::from_secs(10));
    }

    /// A failing child's `GitCommand` error keeps the display tail bounded
    /// while the classification field carries the complete stderr: an
    /// early refusal line must survive the 512-byte tail cut
    /// (docs/implementation/07 §Git adapter). Unix-gated: the fixture is
    /// `sh`.
    #[cfg(unix)]
    #[test]
    fn run_child_keeps_the_full_stderr_beside_the_bounded_tail() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("echo refusal-line >&2; head -c 600 /dev/zero | tr '\\0' x >&2; exit 1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        let error = super::run_child(
            &mut child,
            "git fetch origin".to_owned(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap_err();

        match error {
            IngestError::GitCommand {
                stderr,
                full_stderr,
                ..
            } => {
                assert!(stderr.len() <= 512);
                // The fixture's refusal line sits before 600 bytes of
                // padding, so the tail alone cannot classify it.
                assert!(
                    !stderr.contains("refusal-line"),
                    "fixture broken: the tail must not carry the refusal"
                );
                assert!(full_stderr.starts_with("refusal-line\n"));
                assert!(full_stderr.len() > 600);
            }
            other => panic!("expected GitCommand, got {other:?}"),
        }
    }

    /// Clears the process-global shutdown flag on drop: a panic inside a
    /// shutdown test must not leave the flag set and poison concurrent
    /// `run_child` tests.
    struct ShutdownGuard;

    impl Drop for ShutdownGuard {
        fn drop(&mut self) {
            super::SHUTDOWN.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// A shutdown request stops the wait loop while the child still runs:
    /// `run_child` kills and reaps it promptly instead of letting it run
    /// to completion (unix-gated: the hanging fixture is `sleep`).
    #[cfg(unix)]
    #[test]
    fn run_child_kills_a_child_after_a_shutdown_request() {
        // The flag is process-global: take the shared test lock so no
        // concurrent run_child test polls while it is set.
        let _lock = crate::git::test_util::TEMP_CLONE_ORDER.lock().unwrap();
        // Clear any flag left by a prior run; the guard restores a clear
        // flag on every exit path, panics included.
        let _guard = ShutdownGuard;
        super::SHUTDOWN.store(false, std::sync::atomic::Ordering::SeqCst);
        super::request_shutdown();
        let mut command = Command::new("sleep");
        command
            .arg("30")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0);
        let mut child = command.spawn().unwrap();

        let start = Instant::now();
        let error = super::run_child(
            &mut child,
            "git clone slow".to_owned(),
            Instant::now() + Duration::from_secs(20),
        )
        .unwrap_err();
        let elapsed = start.elapsed();

        // The child must be reaped long before its 30 s runtime or the
        // 20 s deadline: the shutdown fires within one poll interval.
        assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
        assert!(child.try_wait().unwrap().is_some());
        match error {
            IngestError::GitTimeout { .. } => {}
            other => panic!("expected GitTimeout, got {other:?}"),
        }
    }
}
