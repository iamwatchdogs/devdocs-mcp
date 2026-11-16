// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(unix)]
use crate::subprocess::{env_binary, from_target_dir_in, path_binary};
use crate::subprocess::{locate_devdocs, missing_binary_error};

#[cfg(unix)]
use super::stub;

#[test]
fn an_explicit_binary_is_used_as_given() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let bin = stub(dir.path(), "devdocs", "#!/bin/sh\nexit 0\n");
    #[cfg(not(unix))]
    let bin = {
        let bin = dir.path().join("devdocs.exe");
        std::fs::write(&bin, b"stub").unwrap();
        bin
    };
    assert_eq!(locate_devdocs(Some(&bin)).unwrap(), bin);

    let missing = dir.path().join("absent");
    let err = locate_devdocs(Some(&missing)).unwrap_err();
    assert!(err.to_string().contains("is not a file"), "{err}");
}

#[cfg(unix)]
#[test]
fn the_env_override_resolves_a_real_file_only() {
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), "devdocs", "#!/bin/sh\nexit 0\n");

    assert_eq!(env_binary(Some(bin.to_str().unwrap())).unwrap(), bin);
    // An empty or nonexistent override resolves to nothing.
    assert_eq!(env_binary(Some("")), None);
    assert_eq!(
        env_binary(Some(dir.path().join("absent").to_str().unwrap())),
        None
    );
    assert_eq!(env_binary(None), None);
}

#[cfg(unix)]
#[test]
fn the_target_dir_resolver_checks_the_override_then_walks_up() {
    let dir = tempfile::tempdir().unwrap();
    let exe = format!("devdocs{}", std::env::consts::EXE_SUFFIX);

    // The explicit target dir wins when it holds a build, debug first.
    let override_dir = tempfile::tempdir().unwrap();
    let debug_dir = override_dir.path().join("debug");
    std::fs::create_dir_all(&debug_dir).unwrap();
    let debug = stub(&debug_dir, &exe, "#!/bin/sh\nexit 0\n");
    assert_eq!(
        from_target_dir_in(
            Some(override_dir.path().to_str().unwrap()),
            dir.path(),
            &exe
        )
        .unwrap(),
        debug
    );

    // A release-only override is found on the second profile.
    let release_dir_dir = tempfile::tempdir().unwrap();
    let release_dir = release_dir_dir.path().join("release");
    std::fs::create_dir_all(&release_dir).unwrap();
    let release = stub(&release_dir, &exe, "#!/bin/sh\nexit 0\n");
    assert_eq!(
        from_target_dir_in(
            Some(release_dir_dir.path().to_str().unwrap()),
            dir.path(),
            &exe
        )
        .unwrap(),
        release
    );

    // Without a build in the override it falls back to the walk-up
    // from the start directory, which finds the workspace build.
    let workspace = tempfile::tempdir().unwrap();
    let workspace_debug = workspace.path().join("target").join("debug");
    std::fs::create_dir_all(&workspace_debug).unwrap();
    let found = stub(&workspace_debug, &exe, "#!/bin/sh\nexit 0\n");
    let nested = workspace.path().join("crates").join("eval");
    assert_eq!(from_target_dir_in(None, &nested, &exe).unwrap(), found);

    // Without a build anywhere on the walk-up, nothing resolves.
    let bare = tempfile::tempdir().unwrap();
    let empty_override = tempfile::tempdir().unwrap();
    assert_eq!(
        from_target_dir_in(
            Some(empty_override.path().to_str().unwrap()),
            bare.path(),
            &exe
        ),
        None
    );
}

#[cfg(unix)]
#[test]
fn the_path_resolver_finds_a_binary_on_path_only() {
    let dir = tempfile::tempdir().unwrap();
    let exe = format!("devdocs{}", std::env::consts::EXE_SUFFIX);
    let bin = stub(dir.path(), &exe, "#!/bin/sh\nexit 0\n");

    assert_eq!(
        path_binary(std::ffi::OsStr::new(dir.path().to_str().unwrap()), &exe).unwrap(),
        bin
    );
    // A PATH entry without the binary resolves to nothing.
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        path_binary(std::ffi::OsStr::new(empty.path().to_str().unwrap()), &exe),
        None
    );
}

#[test]
fn the_missing_binary_error_names_the_workarounds() {
    let message = missing_binary_error().to_string();
    assert!(
        message.contains("cannot find the devdocs binary"),
        "{message}"
    );
    assert!(message.contains("--devdocs-bin"), "{message}");
    assert!(message.contains("DEVDOCS_EVAL_DEVDOCS_BIN"), "{message}");
}
