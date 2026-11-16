//! The CLI-subprocess pass (docs/implementation/14 §Harness flow): the
//! harness times `devdocs search` end-to-end. The p95 gate is judged on
//! these wall times, cold start excluded after one warmup call.
//!
//! # Security
//!
//! No shell is ever involved: every invocation is one
//! [`std::process::Command`] whose program and arguments travel as a
//! process argument vector (`shell=False` equivalent). No string is ever
//! concatenated into a command line, so no command-injection surface
//! exists; a question text with `; rm -rf /` is passed as one argument.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::EvalError;
use crate::suite::Question;

/// One timed subprocess call.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SubprocessSample {
    /// Wall time in milliseconds.
    pub wall_ms: f64,
    /// The process exit code; -1 when the process died to a signal.
    pub exit_code: i32,
}

/// Locates the `devdocs` binary: the explicit argument, then
/// `DEVDOCS_EVAL_DEVDOCS_BIN`, then a `target/{debug,release}/devdocs`
/// walk up from the working directory (honoring `CARGO_TARGET_DIR` and the
/// platform exe suffix), then `PATH`.
///
/// # Errors
/// [`EvalError::Subprocess`] when nothing resolves to an executable file.
pub fn locate_devdocs(explicit: Option<&Path>) -> Result<PathBuf, EvalError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(EvalError::Subprocess(format!(
                "--devdocs-bin {} is not a file",
                path.display()
            )));
        }
        return Ok(path.to_path_buf());
    }
    let exe = format!("devdocs{}", std::env::consts::EXE_SUFFIX);
    from_env()
        .or_else(|| from_target_dir(&exe))
        .or_else(|| from_path(&exe))
        .ok_or_else(missing_binary_error)
}

/// The resolver-exhausted error: it names every workaround the caller has.
fn missing_binary_error() -> EvalError {
    EvalError::Subprocess(
        "cannot find the devdocs binary; pass --devdocs-bin, set \
         DEVDOCS_EVAL_DEVDOCS_BIN, or run `cargo build -p devdocs` first"
            .to_owned(),
    )
}

/// The `DEVDOCS_EVAL_DEVDOCS_BIN` override.
fn from_env() -> Option<PathBuf> {
    env_binary(std::env::var("DEVDOCS_EVAL_DEVDOCS_BIN").ok().as_deref())
}

/// [`from_env`] on an injected value, so the tests never touch the process
/// environment.
fn env_binary(value: Option<&str>) -> Option<PathBuf> {
    let from_env = value?;
    if from_env.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(from_env);
    candidate.is_file().then_some(candidate)
}

/// The workspace target directory: `CARGO_TARGET_DIR` when set, else a
/// `target/{debug,release}` walk up from the working directory.
fn from_target_dir(exe: &str) -> Option<PathBuf> {
    let target_dir = std::env::var("CARGO_TARGET_DIR").ok();
    let start = std::env::current_dir().ok()?;
    from_target_dir_in(target_dir.as_deref(), &start, exe)
}

/// [`from_target_dir`] on injected values, so the tests never touch the
/// process environment or working directory.
fn from_target_dir_in(target_dir: Option<&str>, start: &Path, exe: &str) -> Option<PathBuf> {
    let profiles = ["debug", "release"];
    if let Some(target_dir) = target_dir
        && !target_dir.is_empty()
    {
        for profile in profiles {
            let candidate = Path::new(target_dir).join(profile).join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut dir = start.to_path_buf();
    loop {
        for profile in profiles {
            let candidate = dir.join("target").join(profile).join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// A `devdocs` on `PATH`.
fn from_path(exe: &str) -> Option<PathBuf> {
    path_binary(&std::env::var_os("PATH")?, exe)
}

/// [`from_path`] on an injected `PATH`, so the tests never touch the
/// process environment.
fn path_binary(path_var: &std::ffi::OsStr, exe: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

/// Whether a variable name carries the `DEVDOCS_` prefix. Windows
/// environment names are case-insensitive but case-preserving
/// (`GetEnvironmentVariableW`), so a lowercase-prefixed `devdocs_*`
/// variable still resolves for the child and must match the strip.
#[cfg(windows)]
fn is_devdocs_env_name(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes()
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"DEVDOCS_"))
}

/// Non-Windows: environment names are case-sensitive, so the byte prefix
/// test is exact.
#[cfg(not(windows))]
fn is_devdocs_env_name(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes().starts_with(b"DEVDOCS_")
}

/// Removes every `DEVDOCS_*` variable from the child environment.
///
/// The env config layer merges *after* the `--config` layer
/// (devdocs-model `Config::load`), so a `DEVDOCS_*` variable that leaks
/// from the operator's environment would silently override the corpus
/// config pin. The child needs no `DEVDOCS_*` variable, so all of them go.
fn strip_devdocs_env(
    command: &mut Command,
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) {
    // OsString, not String: `env::vars()` panics on any non-UTF-8
    // variable, so the scan itself must run on raw bytes (the same
    // pattern the testkit `GIT_*` strip uses).
    for (name, _) in vars.filter(|(name, _)| is_devdocs_env_name(name)) {
        command.env_remove(name);
    }
}

/// The wall-time deadline one `devdocs search` call gets before the run
/// treats it as stalled.
const CALL_DEADLINE: Duration = Duration::from_secs(30);

/// The `try_wait` poll interval while a call runs.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Times one `devdocs search` invocation end-to-end: process start
/// through retrieval (docs/implementation/05 §Latency budget).
///
/// The program and every argument travel as an argument vector — no shell,
/// no interpolation (see the module security note). The call gets a fixed
/// deadline: a stalled child is killed and reported as an error instead of
/// blocking the run forever. The output is not captured — the harness
/// reads only the timing and the exit code.
///
/// # Errors
/// [`EvalError::Subprocess`] when the process cannot be spawned, cannot be
/// waited on, or overshoots the deadline.
pub fn run_once(
    bin: &Path,
    db: &Path,
    config: &Path,
    config_home: &Path,
    question: &Question,
) -> Result<SubprocessSample, EvalError> {
    run_once_deadline(bin, db, config, config_home, question, CALL_DEADLINE)
}

/// [`run_once`] with an injectable deadline; the tests shrink it so the
/// kill path runs in milliseconds instead of the full 30 seconds.
fn run_once_deadline(
    bin: &Path,
    db: &Path,
    config: &Path,
    config_home: &Path,
    question: &Question,
    deadline: Duration,
) -> Result<SubprocessSample, EvalError> {
    let start = Instant::now();
    let mut command = Command::new(bin);
    command
        .arg("--db")
        .arg(db)
        // A config file always rides along — the operator `--config`, or
        // the harness's empty defaults file — and the child's user-config
        // layer points at the harness's empty scratch dir, so the spawned
        // search never grades against the operator's user config and
        // never discovers a project `devdocs.toml` from the inherited cwd
        // (hermetic, docs/implementation/14 §Regression gate).
        .arg("--config")
        .arg(config)
        .arg("--json")
        .arg("search");
    strip_devdocs_env(&mut command, std::env::vars_os());
    command.env("XDG_CONFIG_HOME", config_home);
    let mut child = command
        .arg(&question.question)
        .arg("--library")
        .arg(&question.library)
        .arg("--version")
        .arg(&question.version)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| EvalError::Subprocess(format!("cannot spawn {}: {e}", bin.display())))?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                return Ok(SubprocessSample {
                    wall_ms,
                    exit_code: status.code().unwrap_or(-1),
                });
            }
            Ok(None) if start.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EvalError::Subprocess(format!(
                    "devdocs search exceeded the {}s deadline; the process was killed",
                    deadline.as_secs()
                )));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(source) => {
                return Err(EvalError::Subprocess(format!(
                    "cannot wait for {}: {source}",
                    bin.display()
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests;
