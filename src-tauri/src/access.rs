//! Session allowlist of paths the user explicitly handed to RADIAL.
//!
//! The webview can only name files and folders that arrived through a native
//! picker, drag-and-drop, launch arguments, the user's saved default folder,
//! or that RADIAL itself created. Anything else is refused, so a compromised
//! or buggy frontend cannot read or write arbitrary locations.

use radial_core::{ErrorKind, RadialError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Access {
    files: HashSet<PathBuf>,
    dirs: HashSet<PathBuf>,
}

/// Canonical form used for comparisons; falls back to the path as given.
fn key(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

impl Access {
    /// Allow reading `file` and writing next to it (the default destination).
    pub fn grant_file(&mut self, file: &Path) {
        self.files.insert(key(file));
        if let Some(parent) = file.parent() {
            self.dirs.insert(key(parent));
        }
    }

    pub fn grant_dir(&mut self, dir: &Path) {
        self.dirs.insert(key(dir));
    }

    pub fn check_file(&self, file: &Path) -> Result<()> {
        if self.files.contains(&key(file)) {
            Ok(())
        } else {
            Err(denied())
        }
    }

    pub fn check_dir(&self, dir: &Path) -> Result<()> {
        if self.dirs.contains(&key(dir)) {
            Ok(())
        } else {
            Err(denied())
        }
    }
}

fn denied() -> RadialError {
    RadialError::new(
        ErrorKind::PathNotAllowed,
        "RADIAL can only use files and folders you chose. Open the file again to continue.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_are_scoped() {
        let dir = tempfile_dir();
        let f = dir.join("a.png");
        std::fs::write(&f, b"x").unwrap();
        let mut a = Access::default();
        assert!(a.check_file(&f).is_err());
        a.grant_file(&f);
        assert!(a.check_file(&f).is_ok());
        assert!(
            a.check_dir(&dir).is_ok(),
            "parent of a granted file is a valid destination"
        );
        assert!(a.check_file(&dir.join("b.png")).is_err());
        assert!(a.check_dir(Path::new("/")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("radial-access-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
