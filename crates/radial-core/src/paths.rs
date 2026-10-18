//! Source and destination path validation.

use crate::error::{ErrorKind, RadialError, Result};
use std::path::{Path, PathBuf};

/// Validate that `path` names an existing regular file.
pub fn validate_source(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(RadialError::new(
            ErrorKind::InvalidFile,
            "Expected an absolute file path.",
        ));
    }
    let meta =
        std::fs::metadata(path).map_err(|e| RadialError::from_io(&e, "opening the source file"))?;
    if !meta.is_file() {
        return Err(RadialError::new(
            ErrorKind::InvalidFile,
            "The source is not a regular file.",
        ));
    }
    Ok(path.to_path_buf())
}

/// Validate that `dir` is an existing directory RADIAL can create files in.
/// Writability is checked by actually creating (and removing) a temp file,
/// which is the only reliable test across platforms and ACLs.
pub fn validate_destination_dir(dir: &Path) -> Result<PathBuf> {
    if !dir.is_absolute() {
        return Err(RadialError::new(
            ErrorKind::DestinationNotWritable,
            "Expected an absolute folder path.",
        ));
    }
    let meta = std::fs::metadata(dir).map_err(|e| {
        let mut err = RadialError::from_io(&e, "opening the destination folder");
        if err.kind == ErrorKind::NotFound {
            err = RadialError::new(
                ErrorKind::DestinationNotWritable,
                "The destination folder no longer exists.",
            );
        }
        err
    })?;
    if !meta.is_dir() {
        return Err(RadialError::new(
            ErrorKind::DestinationNotWritable,
            "The destination is not a folder.",
        ));
    }
    tempfile::Builder::new()
        .prefix(".radial-probe-")
        .tempfile_in(dir)
        .map_err(|e| {
            let mut err = RadialError::from_io(&e, "writing to the destination folder");
            if err.kind == ErrorKind::PermissionDenied {
                err = RadialError::new(
                    ErrorKind::DestinationNotWritable,
                    "RADIAL cannot write to that folder. Choose another destination.",
                );
            }
            err
        })?;
    Ok(dir.to_path_buf())
}

/// Whether two paths refer to the same file. Falls back to a lexical,
/// case-insensitive-on-Windows comparison when either does not exist yet.
pub fn same_file_path(a: &Path, b: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (a.canonicalize(), b.canonicalize()) {
        return ca == cb;
    }
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_must_be_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            validate_source(dir.path()).unwrap_err().kind,
            ErrorKind::InvalidFile
        );
        assert_eq!(
            validate_source(&dir.path().join("nope.png"))
                .unwrap_err()
                .kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            validate_source(Path::new("relative.png")).unwrap_err().kind,
            ErrorKind::InvalidFile
        );
        let f = dir.path().join("ok.png");
        std::fs::write(&f, b"x").unwrap();
        assert!(validate_source(&f).is_ok());
    }

    #[test]
    fn destination_must_be_writable_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_destination_dir(dir.path()).is_ok());
        let missing = dir.path().join("missing");
        assert_eq!(
            validate_destination_dir(&missing).unwrap_err().kind,
            ErrorKind::DestinationNotWritable
        );
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            validate_destination_dir(&file).unwrap_err().kind,
            ErrorKind::DestinationNotWritable
        );
        // The probe file must not be left behind.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_destination_is_rejected() {
        use std::os::unix::fs::PermissionsExt;
        if is_root() {
            return; // root bypasses permission bits
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = validate_destination_dir(dir.path()).unwrap_err();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(err.kind, ErrorKind::DestinationNotWritable);
    }

    #[cfg(unix)]
    fn is_root() -> bool {
        std::env::var("USER").map(|u| u == "root").unwrap_or(false)
            || std::fs::metadata("/root")
                .map(|m| {
                    std::os::unix::fs::MetadataExt::uid(&m) == 0
                        && std::fs::read_dir("/root").is_ok()
                })
                .unwrap_or(false)
    }
}
