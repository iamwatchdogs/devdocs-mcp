// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::subprocess::strip_devdocs_env;

/// The `DEVDOCS_*` variables the child receives are gone: the unit
/// check drives [`crate::subprocess::strip_devdocs_env`] with a made-up
/// environment, the end-to-end check asserts the recorder stub — which
/// inherits the test process environment through the real spawn —
/// saw none (tests run one per process under nextest, so the test
/// process never carries a stray `DEVDOCS_*` variable that could
/// fake a pass).
#[test]
fn the_spawn_strips_the_devdocs_env_vars() {
    use std::ffi::OsString;
    use std::process::Command as StdCommand;

    let mut command = StdCommand::new("unused");
    strip_devdocs_env(
        &mut command,
        [
            (
                OsString::from("DEVDOCS_SEARCH_RRF_K"),
                OsString::from("123"),
            ),
            (
                OsString::from("DEVDOCS_SEARCH_MAX_TOKENS_DEFAULT"),
                OsString::from("9000"),
            ),
            (OsString::from("PATH"), OsString::from("/bin")),
        ]
        .into_iter(),
    );
    let removed: Vec<_> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        removed,
        vec!["DEVDOCS_SEARCH_MAX_TOKENS_DEFAULT", "DEVDOCS_SEARCH_RRF_K"]
    );
}

/// Windows environment names are case-insensitive but
/// case-preserving, so a lowercase-prefixed variable resolves for the
/// child and the strip must still match it (the unix suite skips
/// this — its names are case-sensitive).
#[cfg(windows)]
#[test]
fn the_spawn_strips_a_mixed_case_devdocs_var() {
    use std::ffi::OsString;
    use std::process::Command as StdCommand;

    let mut command = StdCommand::new("unused");
    strip_devdocs_env(
        &mut command,
        [
            (
                OsString::from("devdocs_SEARCH_RRF_K"),
                OsString::from("123"),
            ),
            (OsString::from("PATH"), OsString::from("/bin")),
        ]
        .into_iter(),
    );
    let removed: Vec<_> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    assert_eq!(removed, vec!["devdocs_SEARCH_RRF_K"]);
}

/// A non-UTF-8 variable — prefixed or not — must not panic the strip:
/// the prefix check runs on raw bytes and the removal keeps the name
/// verbatim (`env::vars()` would abort the whole run instead).
#[cfg(unix)]
#[test]
fn the_strip_survives_non_utf8_env_names() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command as StdCommand;

    let prefixed = OsString::from_vec(b"DEVDOCS_\xff\xfe".to_vec());
    let unrelated = OsString::from_vec(b"OTHER_\xff".to_vec());
    let mut command = StdCommand::new("unused");
    strip_devdocs_env(
        &mut command,
        [
            (prefixed.clone(), OsString::from("1")),
            (unrelated.clone(), OsString::from("x")),
            (OsString::from("PATH"), OsString::from("/bin")),
        ]
        .into_iter(),
    );
    let removed: Vec<_> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_owned())
        .collect();
    assert_eq!(removed, vec![prefixed], "the unrelated var must survive");
}
