//! Safe output creation shared by every operation.
//!
//! 1. [`plan`] validates the destination and refuses to touch the source or an
//!    existing file.
//! 2. [`stage`] creates a hidden temporary file beside the destination (same
//!    volume, so the final move is atomic).
//! 3. The operation writes and verifies the temporary file.
//! 4. [`persist`] moves it into place without overwriting anything.
//!
//! Dropping a staged file (error, cancel) deletes it, so a failed operation
//! never leaves something that looks like a finished result.

use crate::error::{ErrorKind, RadialError, Result};
use crate::naming;
use crate::paths;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Validate the destination folder and name, returning the final path.
pub fn plan(source: &Path, dest_dir: &Path, stem: &str, extension: &str) -> Result<PathBuf> {
    let stem = naming::validate_stem(stem)?;
    let dir = paths::validate_destination_dir(dest_dir)?;
    let out = dir.join(format!("{stem}.{extension}"));
    if paths::same_file_path(&out, source) {
        return Err(RadialError::new(
            ErrorKind::DestinationExists,
            "The output would replace the original file.",
        ));
    }
    if out.exists() {
        return Err(RadialError::new(
            ErrorKind::DestinationExists,
            format!(
                "“{}” already exists. Choose another name.",
                out.file_name().unwrap_or_default().to_string_lossy()
            ),
        ));
    }
    Ok(out)
}

/// Create a hidden temporary file in the destination's folder.
pub fn stage(out: &Path) -> Result<NamedTempFile> {
    let dir = out.parent().unwrap_or(Path::new("."));
    let mut builder = tempfile::Builder::new();
    builder.prefix(".radial-").suffix(".part");
    // Temp files default to owner-only (0600). The output is an ordinary user
    // file, so request 0666 and let the process umask apply, as `File::create` would.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    builder
        .tempfile_in(dir)
        .map_err(|e| RadialError::from_io(&e, "creating the output file"))
}

/// Flush a staged file to disk.
pub fn sync(tmp: &NamedTempFile) -> Result<()> {
    tmp.as_file()
        .sync_all()
        .map_err(|e| RadialError::from_io(&e, "writing the output"))
}

pub fn staged_size(tmp: &NamedTempFile) -> u64 {
    tmp.as_file().metadata().map(|m| m.len()).unwrap_or(0)
}

/// Move a verified staged file into place. Never overwrites.
pub fn persist(tmp: NamedTempFile, out: &Path) -> Result<u64> {
    let file = tmp.persist_noclobber(out).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            RadialError::new(
                ErrorKind::DestinationExists,
                "A file with that name appeared while RADIAL was working. Nothing was overwritten.",
            )
        } else {
            RadialError::from_io(&e.error, "saving the output")
        }
    })?;
    Ok(file.metadata().map(|m| m.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn plan_refuses_source_and_existing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        std::fs::write(&src, b"x").unwrap();
        assert_eq!(
            plan(&src, dir.path(), "a", "png").unwrap_err().kind,
            ErrorKind::DestinationExists
        );
        assert!(plan(&src, dir.path(), "b", "png").is_ok());
    }

    #[test]
    fn dropped_stage_leaves_nothing_and_persist_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.txt");
        drop(stage(&out).unwrap());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let mut t = stage(&out).unwrap();
        t.write_all(b"new").unwrap();
        std::fs::write(&out, b"old").unwrap();
        assert_eq!(
            persist(t, &out).unwrap_err().kind,
            ErrorKind::DestinationExists
        );
        assert_eq!(std::fs::read(&out).unwrap(), b"old");
    }
}
