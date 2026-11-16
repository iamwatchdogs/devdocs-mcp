// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::suite::{Grader, Question, TaskType};
#[cfg(unix)]
use devdocs_testkit::sh_quote;

mod env;
mod locate;
#[cfg(unix)]
mod recorder;
mod spawn;

// The stub helpers below are the only PathBuf users, and all of them are unix-only.
#[cfg(unix)]
use std::path::PathBuf;

/// A one-question stand-in for the suite loader.
fn question() -> Question {
    Question {
        id: "q-001".to_owned(),
        library: "test-lib".to_owned(),
        version: "1.0.0".to_owned(),
        task_type: TaskType::ExactApi,
        question: "middleware".to_owned(),
        expect_refs: Some(vec!["docs://test-lib/v1.0.0/proxy".to_owned()]),
        expect_symbols: None,
        grader: Grader::RefMatch,
        notes: "n".to_owned(),
    }
}

/// A stand-in binary: a script whose behavior the test controls.
#[cfg(unix)]
fn stub(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join(name);
    std::fs::write(&bin, body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// A stub that records its argument vector, one argument per line,
/// every `DEVDOCS_*` variable its environment still carries (one
/// `NAME=value` per line; the file exists but stays empty when the
/// environment carries none), and the `XDG_CONFIG_HOME` value it sees
/// (the file exists but stays empty when the variable is unset).
#[cfg(unix)]
fn argv_recorder(dir: &std::path::Path, name: &str) -> PathBuf {
    let args_file = dir.join(format!("{name}.args"));
    let env_file = dir.join(format!("{name}.env"));
    let xdg_file = dir.join(format!("{name}.xdg"));
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nenv | grep '^DEVDOCS_' > \
         {}\nprintf '%s\\n' \"${{XDG_CONFIG_HOME-}}\" > {}\nexit 0\n",
        sh_quote(&args_file),
        sh_quote(&env_file),
        sh_quote(&xdg_file)
    );
    stub(dir, name, &body);
    args_file
}

/// The `DEVDOCS_*` environment file the [`argv_recorder`] stub writes.
#[cfg(unix)]
fn recorded_env(dir: &std::path::Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.env"))
}

/// The `XDG_CONFIG_HOME` file the [`argv_recorder`] stub writes.
#[cfg(unix)]
fn recorded_xdg(dir: &std::path::Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.xdg"))
}
