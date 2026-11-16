// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::error::EvalError;
use crate::process_lock;
use crate::run::{
    RunOptions, child_config_file, discover_eval_root, fresh_run_dir, hash_file, resolve_plan, run,
};
use devdocs_retrieve::Plan;

/// The scoring pass resolves the same search knobs the timed
/// subprocess loads from the child config file: the file layer plus
/// defaults (the child's isolated env leaves nothing else). A config
/// pinning non-default knobs moves both passes together.
#[test]
fn the_scoring_plan_resolves_from_the_config_file_layer() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("corpus.toml");
    std::fs::write(&config, "[search]\nlimit_default = 3\nrrf_k = 90\n").unwrap();
    let plan = resolve_plan(&config).unwrap();
    assert_eq!(plan.limit, 3);
    assert_eq!(plan.rrf_k, 90);
    // Unspecified knobs stay on the defaults, not the operator's env.
    assert_eq!(plan.max_tokens, 2000);
}

#[test]
fn hashing_a_missing_database_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = hash_file(&dir.path().join("absent.db")).unwrap_err();
    assert!(matches!(err, EvalError::Io { action: "open", .. }), "{err}");
    assert_eq!(err.exit_code(), 3);
}

/// With no operator `--config`, the harness hands every child an
/// empty defaults file from the run dir: the file exists, holds
/// nothing, and resolves to the documented defaults — so the spawn
/// never falls back to the project `devdocs.toml` discovery the
/// inherited cwd would otherwise run.
#[test]
fn the_run_passes_a_defaults_file_when_no_config_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let defaults = child_config_file(None, dir.path()).unwrap();
    assert_eq!(defaults, dir.path().join("child-defaults.toml"));
    assert_eq!(std::fs::read(&defaults).unwrap(), b"");
    // The empty layer scores with the documented defaults, matching
    // what the child loads from the same file.
    assert_eq!(resolve_plan(&defaults).unwrap(), Plan::defaults());
}

/// An operator `--config` is snapshotted into the run dir: the
/// children load the snapshot copy, not the original path, so a
/// mid-run edit of the original cannot change what the run measures.
#[test]
fn the_run_snapshots_the_operator_config_into_the_run_dir() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("corpus.toml");
    std::fs::write(&config, "[search]\nlimit_default = 3\n").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let snapshot = child_config_file(Some(&config), run_dir.path()).unwrap();
    assert_eq!(snapshot, run_dir.path().join("child-config.toml"));
    assert_eq!(
        std::fs::read(&snapshot).unwrap(),
        std::fs::read(&config).unwrap()
    );
    assert!(!run_dir.path().join("child-defaults.toml").exists());
}

#[test]
fn an_uncreatable_runs_root_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let err = fresh_run_dir(&blocker.join("runs"), "2026-11-02T00:00:00Z", "suite").unwrap_err();
    assert!(
        matches!(
            err,
            EvalError::Io {
                action: "create",
                ..
            }
        ),
        "{err}"
    );
}

#[cfg(unix)]
#[test]
fn an_unwritable_runs_root_is_an_io_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let runs = dir.path().join("runs");
    std::fs::create_dir_all(&runs).unwrap();
    std::fs::set_permissions(&runs, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(runs.join(".probe"), "x").is_ok() {
        return; // the process ignores the mode bits; nothing to test
    }
    let err = fresh_run_dir(&runs, "2026-11-02T00:00:00Z", "suite").unwrap_err();
    std::fs::set_permissions(&runs, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(
            err,
            EvalError::Io {
                action: "create",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn discovery_walks_up_to_the_nearest_eval_directory() {
    let _guard = process_lock();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("eval").join("golden")).unwrap();
    std::fs::create_dir_all(dir.path().join("a").join("b")).unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path().join("a").join("b")).unwrap();
    let found = discover_eval_root();
    std::env::set_current_dir(previous).unwrap();
    // The walk reports the real path (symlinks resolved, no verbatim
    // prefix), so compare both sides canonicalized.
    assert_eq!(
        std::fs::canonicalize(found.unwrap()).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap().join("eval")
    );
}

#[test]
fn a_missing_config_file_is_a_config_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("absent.toml");
    // The validation runs before the directory resolution, so the run
    // needs no eval ancestor to reach it.
    let err = run(&RunOptions {
        suite: "pilot".to_owned(),
        db: None,
        golden_dir: None,
        runs_dir: None,
        devdocs_bin: None,
        config: Some(config.clone()),
    })
    .unwrap_err();
    assert_eq!(err.exit_code(), 1, "{err}");
    assert!(matches!(err, EvalError::InvalidConfig { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("config file"), "{message}");
    assert!(message.contains(config.to_str().unwrap()), "{message}");
}

#[test]
fn a_malformed_config_file_is_a_config_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("broken.toml");
    std::fs::write(&config, "not toml [[[").unwrap();
    // The validation runs before the directory resolution, so the run
    // needs no eval ancestor to reach it.
    let err = run(&RunOptions {
        suite: "pilot".to_owned(),
        db: None,
        golden_dir: None,
        runs_dir: None,
        devdocs_bin: None,
        config: Some(config.clone()),
    })
    .unwrap_err();
    assert_eq!(err.exit_code(), 1, "{err}");
    assert!(matches!(err, EvalError::InvalidConfig { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("is invalid"), "{message}");
    assert!(message.contains(config.to_str().unwrap()), "{message}");
}

#[test]
fn a_run_without_an_eval_ancestor_or_golden_dir_is_input_error() {
    let _guard = process_lock();
    let dir = tempfile::tempdir().unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    // The tempdir can sit under an ancestor that has eval/golden, for
    // example a nested worktree. The walk then resolves it, so the
    // missing-ancestor precondition does not hold and the test skips.
    if discover_eval_root().is_some() {
        std::env::set_current_dir(previous).unwrap();
        return;
    }
    let outcome = run(&RunOptions {
        suite: "anything".to_owned(),
        db: None,
        golden_dir: None,
        runs_dir: None,
        devdocs_bin: None,
        config: None,
    });
    std::env::set_current_dir(previous).unwrap();
    let err = outcome.unwrap_err();
    assert_eq!(err.exit_code(), 1, "{err}");
    assert!(
        err.to_string()
            .contains("cannot locate the eval/ directory"),
        "{err}"
    );
}
