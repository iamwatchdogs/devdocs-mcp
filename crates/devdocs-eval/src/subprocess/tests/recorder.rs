// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use crate::subprocess::run_once_deadline;

use super::{argv_recorder, question, recorded_env, recorded_xdg};

#[cfg(unix)]
#[test]
fn the_spawned_child_sees_no_devdocs_env_vars() {
    let dir = tempfile::tempdir().unwrap();
    let _args_file = argv_recorder(dir.path(), "devdocs");
    let bin = dir.path().join("devdocs");
    let db = dir.path().join("fixture.db");
    run_once_deadline(
        &bin,
        &db,
        &dir.path().join("corpus.toml"),
        &dir.path().join("config-home"),
        &question(),
        Duration::from_secs(1),
    )
    .unwrap();
    let env = std::fs::read_to_string(recorded_env(dir.path(), "devdocs")).unwrap();
    assert!(env.is_empty(), "the child saw DEVDOCS_* variables: {env}");
}

#[cfg(unix)]
#[test]
fn the_spawned_child_gets_the_isolated_config_home() {
    let dir = tempfile::tempdir().unwrap();
    let _args_file = argv_recorder(dir.path(), "devdocs");
    let bin = dir.path().join("devdocs");
    let db = dir.path().join("fixture.db");
    let config_home = dir.path().join("config-home");
    run_once_deadline(
        &bin,
        &db,
        &dir.path().join("corpus.toml"),
        &config_home,
        &question(),
        Duration::from_secs(1),
    )
    .unwrap();
    let xdg = std::fs::read_to_string(recorded_xdg(dir.path(), "devdocs")).unwrap();
    assert_eq!(xdg.trim(), config_home.to_str().unwrap(), "{xdg}");
}

/// The recorder stub's redirections survive a path with a space: the
/// generated script single-quotes every interpolated path, so the
/// recorded files still land (and the script does not silently write
/// a stray file into the working directory).
#[cfg(unix)]
#[test]
fn the_recorder_stub_survives_paths_with_spaces() {
    let base = tempfile::tempdir().unwrap();
    let spaced_root = base.path().join("eval test dir");
    std::fs::create_dir_all(&spaced_root).unwrap();
    let dir = tempfile::tempdir_in(&spaced_root).unwrap();
    let _args_file = argv_recorder(dir.path(), "devdocs");
    let bin = dir.path().join("devdocs");
    let db = dir.path().join("fixture.db");
    let config_home = dir.path().join("config-home");
    run_once_deadline(
        &bin,
        &db,
        &dir.path().join("corpus.toml"),
        &config_home,
        &question(),
        Duration::from_secs(1),
    )
    .unwrap();
    // All three redirections landed inside the spaced dir with the
    // expected content — nothing split at the space.
    let args = std::fs::read_to_string(dir.path().join("devdocs.args")).unwrap();
    assert!(args.contains("search"), "{args}");
    let env = std::fs::read_to_string(recorded_env(dir.path(), "devdocs")).unwrap();
    assert!(env.is_empty(), "{env}");
    let xdg = std::fs::read_to_string(recorded_xdg(dir.path(), "devdocs")).unwrap();
    assert_eq!(xdg.trim(), config_home.to_str().unwrap(), "{xdg}");
    drop(dir);
}

/// The recorder stub's redirections survive a single quote in the
/// path: the generated script POSIX-escapes every interpolated path
/// (each embedded `'` becomes `'\''`), so the quote cannot terminate
/// a shell word and the recorded files still land.
#[cfg(unix)]
#[test]
fn the_recorder_stub_survives_paths_with_quotes() {
    let base = tempfile::tempdir().unwrap();
    let quoted_root = base.path().join("eval'test dir");
    std::fs::create_dir_all(&quoted_root).unwrap();
    let dir = tempfile::tempdir_in(&quoted_root).unwrap();
    let _args_file = argv_recorder(dir.path(), "devdocs");
    let bin = dir.path().join("devdocs");
    let db = dir.path().join("fixture.db");
    let config_home = dir.path().join("config-home");
    run_once_deadline(
        &bin,
        &db,
        &dir.path().join("corpus.toml"),
        &config_home,
        &question(),
        Duration::from_secs(1),
    )
    .unwrap();
    // All three redirections landed inside the quoted dir with the
    // expected content — nothing split at the quote.
    let args = std::fs::read_to_string(dir.path().join("devdocs.args")).unwrap();
    assert!(args.contains("search"), "{args}");
    let env = std::fs::read_to_string(recorded_env(dir.path(), "devdocs")).unwrap();
    assert!(env.is_empty(), "{env}");
    let xdg = std::fs::read_to_string(recorded_xdg(dir.path(), "devdocs")).unwrap();
    assert_eq!(xdg.trim(), config_home.to_str().unwrap(), "{xdg}");
    drop(dir);
}

/// Every spawn carries `--config <path>`: the operator `--config`, or
/// the harness's empty defaults file when none is given. The path need
/// not exist for the argument vector itself — the harness always
/// writes or validates its file before the spawn.
#[cfg(unix)]
#[test]
fn the_spawn_arg_vector_always_carries_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let args_file = argv_recorder(dir.path(), "devdocs");
    let bin = dir.path().join("devdocs");
    let db = dir.path().join("fixture.db");
    let config = dir.path().join("corpus.toml");
    run_once_deadline(
        &bin,
        &db,
        &config,
        &dir.path().join("config-home"),
        &question(),
        Duration::from_secs(1),
    )
    .unwrap();
    let args = std::fs::read_to_string(&args_file).unwrap();
    assert!(args.contains("--config\n"), "{args}");
    assert!(args.contains(config.to_str().unwrap()), "{args}");
}
