//! The git source adapter: pinned clones, a docs walk, and temp-dir
//! cleanup (docs/implementation/07 §Git adapter).
//!
//! Every `git` invocation is an argv vector against the system binary —
//! no shell, no interpolated command line — with the subprocess-safety
//! hardening of docs/implementation/09 §Git subprocess safety: the
//! ambient `GIT_*` location and control variables stripped, prompting and
//! the system config disabled, and hooks and LFS filters disabled per
//! command. The clone lands in a mode-0700 directory under
//! `<data_root>/tmp` that a `Drop` guard removes, and a SIGINT/SIGTERM
//! handler (installed once per process, idempotent) removes the
//! registered path before exiting 130.
//!
//! The adapter is synchronous; the ingest pipeline wraps it in
//! `spawn_blocking` (docs/implementation/07 §Git adapter).

mod clone;
mod command;
mod temp;
mod walk;

pub(crate) use clone::is_commit_sha;
pub use clone::{GitFetch, GitPlan, PinKind, plan};
pub use temp::{TempClone, install_signal_cleanup};

/// Test-only helpers shared by the submodule test modules: the default
/// extension allowlist, an argv-only fixture git runner, and the lock
/// that serializes the tests creating a `TempClone`.
#[cfg(test)]
pub(crate) mod test_util {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::path::Path;
    use std::process::Command as StdCommand;
    use std::sync::Mutex;

    use devdocs_model::config::DEFAULT_GIT_EXTENSIONS;

    /// Serializes the tests that create a `TempClone`: the clone-path
    /// registry is one process-global slot, so parallel `cargo test`
    /// threads would overwrite each other's registration. nextest runs
    /// each test in its own process and never contends.
    pub(crate) static TEMP_CLONE_ORDER: Mutex<()> = Mutex::new(());

    /// The default extension allowlist as `Vec<String>`, the shape the
    /// config's `ingest.git_extensions` carries.
    pub(crate) fn default_extensions() -> Vec<String> {
        DEFAULT_GIT_EXTENSIONS
            .iter()
            .map(|ext| (*ext).to_owned())
            .collect()
    }

    /// Runs `git` against a fixture with the same argv-only, pinned-env
    /// discipline the testkit uses, returning trimmed stdout.
    pub(crate) fn fixture_git(dir: &Path, args: &[&str]) -> String {
        let mut command = StdCommand::new("git");
        // A hook process inherits GIT_DIR, GIT_INDEX_FILE and friends;
        // drop them so the child only sees the fixture repository.
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                command.env_remove(&key);
            }
        }
        let output = command
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
}
