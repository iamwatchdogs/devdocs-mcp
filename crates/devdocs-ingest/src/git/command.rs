//! The git command builder and runner: every invocation is one argv
//! vector against the system binary with a hardened child env
//! (docs/implementation/09 §Git subprocess safety).

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::IngestError;

/// The `GIT_*` environment variables every invocation strips, so an
/// ambient value cannot redirect the child to another repository or
/// object store, inject config, or hijack prompting and SSH
/// (docs/implementation/09 §Git subprocess safety). Removing
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

/// Builds the command one `git` invocation runs: `git`, the per-command
/// `-c` config (base plus extras), then `args`, from `cwd`. The child env
/// is hardened here: the prompt and system config pins, then the removal
/// of every [`STRIPPED_ENV`] variable so an ambient value cannot reach
/// the git child (docs/implementation/09 §Git subprocess safety).
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
    // never read.
    let stdout_reader = read_pipe(child.stdout.take());
    let stderr_reader = read_pipe(child.stderr.take());
    match wait_until(child, deadline) {
        Ok(Some(status)) => {
            let stdout = join_reader(stdout_reader);
            let stderr = join_reader(stderr_reader);
            if !status.success() {
                return Err(IngestError::GitCommand {
                    command: command_text,
                    stderr: stderr_tail(&stderr),
                });
            }
            Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
        }
        Ok(None) => {
            kill_and_reap(child);
            join_reader(stdout_reader);
            join_reader(stderr_reader);
            Err(IngestError::GitTimeout {
                command: command_text,
                seconds: crate::pipeline::TOTAL_INGEST_CAP.as_secs(),
            })
        }
        Err(error) => {
            kill_and_reap(child);
            join_reader(stdout_reader);
            join_reader(stderr_reader);
            Err(IngestError::GitSpawn {
                message: error.to_string(),
            })
        }
    }
}

/// Waits until the child exits or the deadline passes: `Ok(Some(status))`
/// on exit, `Ok(None)` at expiry — the caller kills and reaps.
fn wait_until(
    child: &mut Child,
    deadline: Instant,
) -> io::Result<Option<std::process::ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(WAIT_POLL);
    }
}

/// Kills the direct child and reaps it so no git process outlives the
/// deadline. git's transport helpers observe the closed pipe and exit on
/// their next write (docs/implementation/09 §Git subprocess safety).
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads one pipe to end on a worker thread.
fn read_pipe<R: io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    })
}

/// Joins a pipe reader; a panicked reader yields an empty buffer.
fn join_reader(reader: std::thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    reader.join().unwrap_or_default()
}

/// The bounded stderr tail for an error: the last 512 characters, cut on
/// a char boundary. No full dump.
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
    /// transport is pinned noninteractive — what an operator shell may
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
            "GIT_STDERR",
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
}
