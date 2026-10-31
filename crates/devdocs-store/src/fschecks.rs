//! Filesystem checks run at `Store::open`.
//!
//! Contract: docs/implementation/09 §Local storage controls.

use std::fs;
use std::path::Path;

use crate::error::StoreError;

/// Ownership is checked against the effective user.
#[cfg(unix)]
// FFI is required for geteuid(2); std exposes no effective-uid read
// (issue #5, docs/implementation/09 §Local storage controls).
#[allow(unsafe_code)]
fn effective_uid() -> u32 {
    // SAFETY: geteuid(2) is thread-safe, infallible, and reads only the
    // process credentials.
    unsafe { libc::geteuid() }
}

/// Whether `file_uid` names another user's territory.
#[cfg(unix)]
fn is_foreign_owner(file_uid: u32) -> bool {
    file_uid != effective_uid()
}

/// Refuses `path` when it is a symlink that resolves to a file the current
/// user does not own.
///
/// A symlink into another user's territory stays an error even with
/// `StoreOptions::allow_network_fs`, because that option only covers the
/// network-filesystem refusal (docs/implementation/09).
#[cfg(unix)]
pub(crate) fn check_symlink_owner(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::MetadataExt;

    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        // A fresh database has no file yet, so no symlink either.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    if !meta.is_symlink() {
        return Ok(());
    }
    let resolved = path.canonicalize().map_err(|source| StoreError::Io {
        path: path.to_owned(),
        source,
    })?;
    let meta = fs::metadata(&resolved).map_err(|source| StoreError::Io {
        path: resolved.clone(),
        source,
    })?;
    if is_foreign_owner(meta.uid()) {
        return Err(StoreError::ForeignSymlink {
            path: path.to_owned(),
        });
    }
    Ok(())
}

/// Non-Unix platforms carry no POSIX symlink-ownership semantics to check.
#[cfg(not(unix))]
pub(crate) fn check_symlink_owner(path: &Path) -> Result<(), StoreError> {
    let _ = path;
    Ok(())
}

/// Refuses `path` when the filesystem holding it is a known network mount.
#[cfg(target_os = "macos")]
pub(crate) fn check_network_mount(path: &Path) -> Result<(), StoreError> {
    let mount = statfs_of(path)?;
    // MNT_LOCAL marks filesystems stored locally; without it the mount is
    // network-backed (automounts, SMB, NFS, ...).
    if mount.f_flags & libc::MNT_LOCAL as u32 == 0 {
        return Err(StoreError::NetworkFsPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

/// Refuses `path` when the filesystem holding it is a known network mount.
#[cfg(target_os = "linux")]
pub(crate) fn check_network_mount(path: &Path) -> Result<(), StoreError> {
    // Magic numbers from linux/magic.h via statfs(2), widened to i64:
    // `f_type` is `__fsword_t` (i64 on 64-bit, i32 on 32-bit), so the
    // widening is loss-free and the 32-bit magic values arrive
    // zero-extended.
    const NFS: i64 = 0x6969;
    const CIFS: i64 = 0xFF53_4D42;
    const SMB2: i64 = 0xFE53_4D42;
    const AFS: i64 = 0x5346_414F;
    const NCP: i64 = 0x564C;
    const CODA: i64 = 0x7375_7245;
    const V9FS: i64 = 0x0102_1997;
    const OCFS2: i64 = 0x7461_636F;

    let mount = statfs_of(path)?;
    // f_type is __fsword_t: already i64 on 64-bit targets, i32 on 32-bit,
    // so the identity-or-widening cast is required to compare in i64 space
    // (issue #5).
    #[allow(clippy::unnecessary_cast)]
    let fs_type = mount.f_type as i64;
    if matches!(fs_type, NFS | CIFS | SMB2 | AFS | NCP | CODA | V9FS | OCFS2) {
        return Err(StoreError::NetworkFsPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

/// No portable network-mount signal exists on other platforms; the check
/// degrades to "assume local" (docs/implementation/09 scopes the control to
/// known network mounts).
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn check_network_mount(path: &Path) -> Result<(), StoreError> {
    let _ = path;
    Ok(())
}

/// `statfs` on the deepest existing ancestor of `path` — the DB file when it
/// exists, its parent directory when it does not (a fresh database has no
/// file yet).
#[cfg(any(target_os = "macos", target_os = "linux"))]
// FFI is required for statfs(2); std exposes no filesystem-identity read
// (issue #5, docs/implementation/09 §Local storage controls).
#[allow(unsafe_code)]
fn statfs_of(path: &Path) -> Result<libc::statfs, StoreError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let target = if path.exists() {
        path.to_owned()
    } else {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.exists() {
            return Err(StoreError::Io {
                path: path.to_owned(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "database directory does not exist",
                ),
            });
        }
        parent.to_owned()
    };

    // SAFETY: `c_path` outlives the call, and `buf` is a valid, initialized
    // statfs buffer that statfs(2) fills in.
    let c_path = CString::new(target.as_os_str().as_bytes()).map_err(|_| StoreError::Io {
        path: path.to_owned(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains an interior NUL",
        ),
    })?;
    let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above; statfs writes only into `buf` on success.
    let rc = unsafe { libc::statfs(c_path.as_ptr(), std::ptr::from_mut(&mut buf)) };
    if rc != 0 {
        return Err(StoreError::Io {
            path: target,
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(buf)
}

/// Fails with the path attached when the database directory does not exist.
///
/// Platform-neutral: without it, non-Unix open paths surface the missing
/// directory as a SQLite `CannotOpen` error instead of a filesystem error.
pub(crate) fn check_parent_exists(path: &Path) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.exists() {
        return Err(StoreError::Io {
            path: path.to_owned(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "database directory does not exist",
            ),
        });
    }
    Ok(())
}

/// Creates the database file with mode 0600 when this call creates it.
///
/// rusqlite would create it with `0666 & !umask`; the security contract
/// wants 0600 (docs/implementation/09 §Local storage controls). An existing
/// file keeps its mode, and a concurrent first opener that won the
/// `create_new` race is treated as success — it created the file 0600 too.
#[cfg(unix)]
pub(crate) fn ensure_private_file(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::OpenOptionsExt;

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(StoreError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Non-Unix platforms have no file mode to enforce at creation.
#[cfg(not(unix))]
pub(crate) fn ensure_private_file(path: &Path) -> Result<(), StoreError> {
    let _ = path;
    Ok(())
}

/// Warns when the file or its directory drifted from the 0600/0700 contract.
///
/// Verification warns rather than refuses (docs/implementation/09: "The
/// store verifies modes at open and warns on drift").
#[cfg(unix)]
pub(crate) fn warn_on_mode_drift(path: &Path) {
    use std::os::unix::fs::MetadataExt;
    use tracing::warn;

    if let Ok(meta) = fs::metadata(path) {
        let mode = meta.mode() & 0o777;
        if mode != 0o600 {
            warn!(
                file = %path.display(),
                mode = format_args!("{mode:04o}"),
                "database file mode drifted from 0600"
            );
        }
    }
    if let Some(parent) = path.parent()
        && let Ok(meta) = fs::metadata(parent)
    {
        let mode = meta.mode() & 0o777;
        if mode != 0o700 {
            warn!(
                directory = %parent.display(),
                mode = format_args!("{mode:04o}"),
                "database directory mode drifted from 0700"
            );
        }
    }
}

/// Non-Unix platforms have no POSIX file mode to compare against.
#[cfg(not(unix))]
pub(crate) fn warn_on_mode_drift(path: &Path) {
    let _ = path;
}
