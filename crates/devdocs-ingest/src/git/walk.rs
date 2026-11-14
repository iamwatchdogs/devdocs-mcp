//! The docs walk over the clone directory: the `ignore` walker with the
//! extension allowlist applied after gitignore
//! (docs/implementation/07 §Git adapter).

use std::path::Path;

use crate::chrome::PageDocument;
use crate::error::IngestError;

/// Walks the clone directory with the `ignore` walker and collects the
/// docs matching the extension allowlist. The walk honors only the
/// clone's own in-tree `.gitignore` and `.ignore` rules
/// (docs/implementation/07 §Git adapter): `require_git(true)` keeps the
/// ignore rules scoped to repositories, `follow_links(false)` never
/// follows a symlink out of the clone, and the host-level sources are
/// pinned off so they cannot hide committed docs.
///
/// The extension allowlist filters in the loop, not through an override
/// whitelist: the `ignore` crate gives overrides the highest precedence —
/// an override match short-circuits before the ignore rules are consulted
/// (ignore 0.4.34 `dir.rs`, `matched_ignore`) — so a `*.md` override
/// would re-admit `.gitignore`d markdown. Filtering after the walk keeps
/// gitignore authoritative over the allowlist. Only UTF-8 decode failures
/// are skipped and counted; a file read that fails at the I/O level fails
/// the walk.
pub(crate) fn walk_docs(
    dir: &Path,
    extensions: &[String],
) -> Result<(Vec<PageDocument>, usize), IngestError> {
    walk_docs_with(dir, extensions, true)
}

/// [`walk_docs`] over a directory that is not a clone root: the fixture
/// replay passes `require_git(false)`, because a materialized tree has no
/// `.git` — with the clone-path pin the walker would drop gitignore
/// handling outside a repository, and files the tree's committed
/// `.gitignore` filters would join the replay corpus
/// (docs/implementation/07 §Git adapter, fixture replay). Every other
/// walker pin stays identical.
pub(crate) fn walk_docs_with(
    dir: &Path,
    extensions: &[String],
    require_git: bool,
) -> Result<(Vec<PageDocument>, usize), IngestError> {
    let allowlist: Vec<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
    // The walker keeps the `ignore` default of skipping hidden entries:
    // dot files and dot directories (`.github/`, `.config/mkdocs.yml`)
    // never reach the allowlist (docs/implementation/07 §Git adapter).
    //
    // The walk honors only the tree's own in-tree `.gitignore` and
    // `.ignore` rules. The host-level sources are pinned off
    // (`parents(false)`, `git_global(false)`, `git_exclude(false)`) for
    // pin determinism: an ancestor `.ignore`, the operator's global
    // `core.excludesFile`, or `.git/info/exclude` would otherwise hide
    // committed docs and make the corpus machine-dependent or empty
    // (`NoIngestablePages`). The global-gitignore path is not covered
    // in-repo: the matcher reads the host git config at runtime.
    let walker = ignore::WalkBuilder::new(dir)
        .require_git(require_git)
        .follow_links(false)
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .build();

    let mut docs = Vec::new();
    let mut skipped_non_utf8 = 0;
    for entry in walker {
        let entry = entry.map_err(|e| IngestError::GitWalk {
            message: format!("walk failed: {e}"),
        })?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        if !allowed_extension(path, &allowlist) {
            continue;
        }
        let relative = path.strip_prefix(dir).unwrap_or(path);
        // Read bytes first: an I/O error (permission denied, EIO) must
        // fail the walk, not masquerade as a skipped non-UTF-8 file —
        // only the decode failure is skippable.
        let bytes = std::fs::read(path).map_err(|e| IngestError::GitWalk {
            message: format!("read failed for {}: {e}", path.display()),
        })?;
        let Ok(body) = String::from_utf8(bytes) else {
            skipped_non_utf8 += 1;
            continue;
        };
        // Chunk paths join URLs and store keys: the separator is `/` on
        // every host, never the platform separator.
        let path_string = relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        docs.push(PageDocument {
            path: path_string,
            body,
            page_url: None,
        });
    }
    Ok((docs, skipped_non_utf8))
}

/// Returns true when the path's extension, compared case-insensitively,
/// is in the allowlist.
fn allowed_extension(path: &Path, allowlist: &[String]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let ext = ext.to_ascii_lowercase();
            allowlist.contains(&ext)
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use tempfile::tempdir;

    use super::super::test_util::default_extensions;
    use super::walk_docs;

    /// The walk honors an untracked `.gitignore` in the fixture
    /// (docs/implementation/07 §Git adapter).
    #[test]
    fn the_walk_honors_gitignore() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();
        std::fs::write(repo.work_dir.join(".gitignore"), "secret.md\n").unwrap();
        std::fs::write(repo.work_dir.join("secret.md"), "# secret\n").unwrap();

        let (docs, skipped) = walk_docs(&repo.work_dir, &default_extensions()).unwrap();
        assert_eq!(skipped, 0);
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"docs/guide.md"), "paths: {paths:?}");
        assert!(!paths.contains(&"secret.md"), "paths: {paths:?}");
    }

    /// A host-level `.ignore` in a parent of the clone root must not
    /// hide committed docs: the walk honors only the clone's own rules
    /// (docs/implementation/07 §Git adapter).
    #[test]
    fn the_walk_ignores_a_parent_ignore_file() {
        // Wrap the clone root so its parent is a controllable temp dir.
        let wrapper = tempdir().unwrap();
        let clone_dir = wrapper.path().join("clone");
        std::fs::create_dir_all(&clone_dir).unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(&clone_dir).unwrap();
        std::fs::write(wrapper.path().join(".ignore"), "*.md\n").unwrap();

        let (docs, skipped) = walk_docs(&repo.work_dir, &default_extensions()).unwrap();
        assert_eq!(skipped, 0);
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"docs/guide.md"), "paths: {paths:?}");
    }

    /// The default allowlist picks up the markdown and txt files of the
    /// fixture; a config-extended allowlist narrows the walk to the
    /// extensions it names (docs/implementation/07 §Git adapter).
    #[test]
    fn the_walk_honors_the_allowlists() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();
        std::fs::write(repo.work_dir.join("notes.txt"), "notes\n").unwrap();
        std::fs::write(repo.work_dir.join("spec.custom"), "custom\n").unwrap();

        // Default allowlist: markdown and txt in, the custom file out.
        let (docs, _) = walk_docs(&repo.work_dir, &default_extensions()).unwrap();
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"notes.txt"), "paths: {paths:?}");
        assert!(!paths.contains(&"spec.custom"), "paths: {paths:?}");

        // A narrowed (config-shaped) allowlist: only the named extensions.
        let (docs, _) = walk_docs(&repo.work_dir, &["txt".to_owned()]).unwrap();
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["notes.txt"]);

        // An extended allowlist: the custom extension joins the set.
        let (docs, _) = walk_docs(&repo.work_dir, &["md".to_owned(), "custom".to_owned()]).unwrap();
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"spec.custom"), "paths: {paths:?}");
        assert!(!paths.contains(&"notes.txt"), "paths: {paths:?}");
    }

    /// Files that fail to read as UTF-8 are skipped and counted
    /// (docs/implementation/07 §Git adapter).
    #[test]
    fn the_walk_skips_non_utf8_files() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();
        std::fs::write(repo.work_dir.join("bin.md"), [0xff, 0xfe, 0x00]).unwrap();

        let (docs, skipped) = walk_docs(&repo.work_dir, &default_extensions()).unwrap();
        assert_eq!(skipped, 1);
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(!paths.contains(&"bin.md"), "paths: {paths:?}");
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
    }

    /// A subdirectory the walk cannot read surfaces as `GitWalk` — the
    /// walker error is the adapter's walk failure, not a silent skip
    /// (docs/implementation/07 §Git adapter). POSIX only: a 000
    /// directory stays readable for its owner on Windows.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_subdir_fails_the_walk() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();
        let locked = repo.work_dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        let error = walk_docs(&repo.work_dir, &default_extensions()).unwrap_err();
        assert!(
            matches!(error, crate::error::IngestError::GitWalk { .. }),
            "{error:?}"
        );

        // Restore so the tempdir cleanup can run.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    /// A file the walk cannot read surfaces as `GitWalk`: an I/O error
    /// must not be skipped and counted as non-UTF-8
    /// (docs/implementation/07 §Git adapter). POSIX only: a 000 file
    /// stays readable for its owner on Windows.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_fails_the_walk() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();
        let locked = repo.work_dir.join("README.md");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }

        let error = walk_docs(&repo.work_dir, &default_extensions()).unwrap_err();
        assert!(
            matches!(error, crate::error::IngestError::GitWalk { .. }),
            "{error:?}"
        );

        // Restore so the tempdir cleanup can run.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
    }

    /// The walk collects the fixture docs under their clone-relative
    /// paths; the path accessor carries forward slashes on every host.
    #[test]
    fn the_walk_reports_clone_relative_paths() {
        let repo_dir = tempdir().unwrap();
        let repo = devdocs_testkit::git_repo::docs_repo(repo_dir.path()).unwrap();

        let (docs, skipped) = walk_docs(&repo.work_dir, &default_extensions()).unwrap();
        assert_eq!(skipped, 0);
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"docs/guide.md"), "paths: {paths:?}");
    }

    /// A materialized tree carries no `.git`. The fixture-replay pin
    /// (`require_git(false)`) honors the tree's in-tree `.gitignore` the
    /// same way the clone walk does inside the repository; the clone-path
    /// pin (`require_git(true)`) would drop gitignore handling outside a
    /// repository, and files the committed `.gitignore` filters would
    /// join the replay corpus
    /// (docs/implementation/07 §Git adapter, fixture replay).
    #[test]
    fn the_replay_pin_honors_gitignore_outside_a_repository() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "secret.md\n").unwrap();
        std::fs::write(dir.path().join("README.md"), "# docs\n").unwrap();
        std::fs::write(dir.path().join("secret.md"), "# secret\n").unwrap();

        let (docs, skipped) =
            super::walk_docs_with(dir.path(), &default_extensions(), false).unwrap();
        assert_eq!(skipped, 0);
        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["README.md"], "the replay honors the gitignore");

        let (unfiltered, _) =
            super::walk_docs_with(dir.path(), &default_extensions(), true).unwrap();
        let paths: Vec<&str> = unfiltered.iter().map(|d| d.path.as_str()).collect();
        assert!(
            paths.contains(&"secret.md"),
            "the clone pin drops gitignore handling outside a repo: {paths:?}"
        );
    }
}
