//! Output file naming: sanitising user-supplied names and avoiding collisions.

use crate::error::{ErrorKind, RadialError, Result};
use crate::format::Format;
use std::path::{Path, PathBuf};

const MAX_STEM_CHARS: usize = 200;

const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a user-entered file stem (no extension). Rejects path separators
/// and names Windows cannot create, rather than silently rewriting them, so the
/// user sees exactly what will be written.
pub fn validate_stem(stem: &str) -> Result<String> {
    let bad = |m: &str| Err(RadialError::new(ErrorKind::InvalidOptions, m));
    let s = stem.trim();
    if s.is_empty() {
        return bad("The file name cannot be empty.");
    }
    if s.chars().count() > MAX_STEM_CHARS {
        return bad("The file name is too long.");
    }
    if s == "." || s == ".." {
        return bad("That file name is not allowed.");
    }
    if let Some(c) = s.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        let shown = if c.is_control() {
            "control characters".to_string()
        } else {
            format!("“{c}”")
        };
        return bad(&format!("File names cannot contain {shown}."));
    }
    if s.ends_with('.') {
        return bad("File names cannot end with a dot.");
    }
    let base = s
        .split('.')
        .next()
        .unwrap_or(s)
        .trim_end()
        .to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&base.as_str()) {
        return bad("That name is reserved by Windows.");
    }
    Ok(s.to_string())
}

/// Make a safe default stem from an arbitrary source file name, replacing
/// characters that cannot appear in file names. Unicode is preserved.
pub fn default_stem(source: &Path) -> String {
    let raw = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut s: String = raw
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(MAX_STEM_CHARS)
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() || validate_stem(&s).is_err() {
        s = format!(
            "converted{}",
            if s.is_empty() {
                String::new()
            } else {
                format!("-{s}")
            }
        );
    }
    validate_stem(&s).unwrap_or_else(|_| "converted".to_string())
}

pub fn file_name(stem: &str, target: Format) -> String {
    format!("{stem}.{}", target.extension())
}

/// First non-existing path in `dir` of the form `stem.ext`, `stem (2).ext`, …
/// Also never returns `avoid` (the source file).
pub fn unique_path(dir: &Path, stem: &str, target: Format, avoid: Option<&Path>) -> PathBuf {
    let same_as_avoid = |p: &Path| avoid.is_some_and(|a| crate::paths::same_file_path(a, p));
    let first = dir.join(file_name(stem, target));
    if !first.exists() && !same_as_avoid(&first) {
        return first;
    }
    for n in 2u32.. {
        let p = dir.join(format!("{stem} ({n}).{}", target.extension()));
        if !p.exists() && !same_as_avoid(&p) {
            return p;
        }
    }
    unreachable!("u32 exhausted looking for a free file name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_stems() {
        assert!(validate_stem("holiday photo").is_ok());
        assert!(validate_stem("фото — 写真 🌅").is_ok());
        assert!(validate_stem("").is_err());
        assert!(validate_stem("   ").is_err());
        assert!(validate_stem("a/b").is_err());
        assert!(validate_stem("a\\b").is_err());
        assert!(validate_stem("..").is_err());
        assert!(validate_stem("CON").is_err());
        assert!(validate_stem("com1.backup").is_err());
        assert!(validate_stem("trailing.").is_err());
        assert!(validate_stem("tab\there").is_err());
        assert!(validate_stem(&"x".repeat(201)).is_err());
    }

    #[test]
    fn default_stem_is_safe() {
        assert_eq!(default_stem(Path::new("/x/My Photo.png")), "My Photo");
        assert_eq!(default_stem(Path::new("/x/写真.jpeg")), "写真");
        assert_eq!(default_stem(Path::new("/x/nul.png")), "converted-nul");
        assert_eq!(default_stem(Path::new("/x/.png")), ".png");
    }

    #[test]
    fn unique_path_skips_existing_and_source() {
        let dir = tempfile::tempdir().unwrap();
        let a = unique_path(dir.path(), "pic", Format::Png, None);
        assert_eq!(a.file_name().unwrap(), "pic.png");
        std::fs::write(&a, b"x").unwrap();
        let b = unique_path(dir.path(), "pic", Format::Png, None);
        assert_eq!(b.file_name().unwrap(), "pic (2).png");
        std::fs::write(&b, b"x").unwrap();
        assert_eq!(
            unique_path(dir.path(), "pic", Format::Png, None)
                .file_name()
                .unwrap(),
            "pic (3).png"
        );

        let src = dir.path().join("photo.jpg");
        let p = unique_path(dir.path(), "photo", Format::Jpeg, Some(&src));
        assert_eq!(p.file_name().unwrap(), "photo (2).jpg");
    }
}
