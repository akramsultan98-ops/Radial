//! Error model shared by every layer of RADIAL.
//!
//! Errors carry a stable machine-readable [`ErrorKind`] (used by the UI to pick
//! a recovery action) and a human-readable message. Messages never include file
//! contents; they may include file names, which the user already knows.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The file type was recognised but RADIAL cannot read or write it.
    UnsupportedFormat,
    /// The requested source → target conversion is not registered.
    UnsupportedConversion,
    /// The file is not what it claims to be, or has no recognisable type.
    InvalidFile,
    /// The file has a valid signature but its contents could not be decoded.
    CorruptFile,
    EmptyFile,
    /// File or decoded image exceeds RADIAL's safety limits.
    TooLarge,
    NotFound,
    PermissionDenied,
    /// The destination file already exists; RADIAL never overwrites silently.
    DestinationExists,
    DestinationNotWritable,
    InsufficientStorage,
    InvalidOptions,
    /// The path was not provided by the user through a picker, drop or launch.
    PathNotAllowed,
    Cancelled,
    /// Compression could not make the file smaller; nothing was written.
    NoReduction,
    /// An optional conversion engine is not installed.
    EngineMissing,
    /// The engine produced output that failed verification, or crashed.
    EngineFailed,
    Io,
    Internal,
}

impl ErrorKind {
    /// Whether retrying the same operation unchanged could plausibly succeed.
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            ErrorKind::Io
                | ErrorKind::InsufficientStorage
                | ErrorKind::DestinationNotWritable
                | ErrorKind::DestinationExists
                | ErrorKind::PermissionDenied
                | ErrorKind::Cancelled
                | ErrorKind::EngineFailed
        )
    }
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
pub struct RadialError {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}

impl fmt::Display for RadialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl RadialError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable: kind.is_retryable(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorKind::Cancelled, "Conversion cancelled.")
    }

    /// Classify an I/O error with a short description of what was being done,
    /// e.g. `"reading the source file"`.
    pub fn from_io(err: &io::Error, doing: &str) -> Self {
        use io::ErrorKind as K;
        let (kind, msg) = match err.kind() {
            K::NotFound => (
                ErrorKind::NotFound,
                format!("File not found while {doing}."),
            ),
            K::PermissionDenied => (
                ErrorKind::PermissionDenied,
                format!("Permission denied while {doing}."),
            ),
            K::AlreadyExists => (
                ErrorKind::DestinationExists,
                "A file with that name already exists. RADIAL will not overwrite it.".to_string(),
            ),
            K::StorageFull | K::QuotaExceeded => (
                ErrorKind::InsufficientStorage,
                format!("Not enough disk space while {doing}."),
            ),
            K::ReadOnlyFilesystem => (
                ErrorKind::DestinationNotWritable,
                format!("The destination is read-only ({doing})."),
            ),
            K::FileTooLarge => (
                ErrorKind::TooLarge,
                format!("File too large while {doing}."),
            ),
            K::UnexpectedEof | K::InvalidData => (
                ErrorKind::CorruptFile,
                format!("The file ended unexpectedly while {doing}."),
            ),
            _ => (
                ErrorKind::Io,
                format!("A file-system error occurred while {doing}: {err}."),
            ),
        };
        Self::new(kind, msg)
    }

    pub fn from_image(err: image::ImageError, doing: &str) -> Self {
        use image::error::{LimitErrorKind, UnsupportedErrorKind};
        use image::ImageError as E;
        match err {
            E::IoError(io) => Self::from_io(&io, doing),
            E::Decoding(_) => Self::new(
                ErrorKind::CorruptFile,
                format!("The image data is damaged or incomplete ({doing})."),
            ),
            E::Encoding(e) => Self::new(ErrorKind::EngineFailed, format!("Encoding failed: {e}.")),
            E::Limits(l) => match l.kind() {
                LimitErrorKind::DimensionError | LimitErrorKind::InsufficientMemory => Self::new(
                    ErrorKind::TooLarge,
                    "The image is too large to process safely on this machine.",
                ),
                _ => Self::new(ErrorKind::TooLarge, format!("Image limit exceeded: {l}.")),
            },
            E::Unsupported(u) => {
                let detail = match u.kind() {
                    UnsupportedErrorKind::Color(c) => format!("color layout {c:?}"),
                    UnsupportedErrorKind::Format(f) => format!("{f}"),
                    UnsupportedErrorKind::GenericFeature(g) => g,
                    _ => "an unsupported feature".to_string(),
                };
                Self::new(
                    ErrorKind::UnsupportedFormat,
                    format!("This image uses {detail}, which RADIAL cannot handle yet."),
                )
            }
            E::Parameter(p) => Self::new(
                ErrorKind::InvalidOptions,
                format!("Invalid parameter: {p}."),
            ),
        }
    }
}

pub type Result<T> = std::result::Result<T, RadialError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_io_errors() {
        let e = RadialError::from_io(&io::Error::from(io::ErrorKind::NotFound), "reading");
        assert_eq!(e.kind, ErrorKind::NotFound);
        let e = RadialError::from_io(&io::Error::from(io::ErrorKind::PermissionDenied), "writing");
        assert_eq!(e.kind, ErrorKind::PermissionDenied);
        assert!(e.retryable);
        let e = RadialError::from_io(&io::Error::from(io::ErrorKind::StorageFull), "writing");
        assert_eq!(e.kind, ErrorKind::InsufficientStorage);
        let e = RadialError::from_io(&io::Error::from(io::ErrorKind::AlreadyExists), "saving");
        assert_eq!(e.kind, ErrorKind::DestinationExists);
        let e = RadialError::from_io(&io::Error::from(io::ErrorKind::UnexpectedEof), "reading");
        assert_eq!(e.kind, ErrorKind::CorruptFile);
        assert!(!e.retryable);
    }

    #[test]
    fn classifies_image_errors() {
        let bad =
            image::load_from_memory_with_format(b"\x89PNG\r\n\x1a\nnope", image::ImageFormat::Png)
                .unwrap_err();
        let e = RadialError::from_image(bad, "decoding");
        assert_eq!(e.kind, ErrorKind::CorruptFile);
    }

    #[test]
    fn serializes_kind_as_snake_case() {
        let e = RadialError::new(ErrorKind::DestinationExists, "x");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"destination_exists\""));
    }
}
