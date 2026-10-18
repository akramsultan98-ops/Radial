//! Content-based file type detection.
//!
//! Detection never trusts the file name. It runs in two steps:
//! 1. [`sniff`] reads magic bytes to identify the container.
//! 2. [`inspect`] asks the matching decoder to parse the header, which proves
//!    the file is *readable* and yields dimensions and colour layout.
//!
//! The extension is only used to warn when it disagrees with the content.

use crate::error::{ErrorKind, RadialError, Result};
use crate::format::{Category, Format, Recognized};
use crate::limits;
use image::ImageDecoder;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

const SNIFF_LEN: usize = 64;

/// Identify a file from its first bytes. Returns `None` for unknown content.
pub fn sniff(head: &[u8]) -> Option<Recognized> {
    use Recognized::{Convertible as C, Known};
    let starts = |sig: &[u8]| head.starts_with(sig);
    let at = |off: usize, sig: &[u8]| {
        head.len() >= off + sig.len() && &head[off..off + sig.len()] == sig
    };

    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some(C(Format::Png));
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some(C(Format::Jpeg));
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(C(Format::Gif));
    }
    if starts(b"II*\0") || starts(b"MM\0*") {
        return Some(C(Format::Tiff));
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some(C(Format::Webp));
    }
    if starts(b"BM") && looks_like_bmp(head) {
        return Some(C(Format::Bmp));
    }

    let known = |id, label, category| {
        Some(Known {
            id,
            label,
            category,
        })
    };
    if starts(b"%PDF-") {
        return known("pdf", "PDF", Category::Document);
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return known("wav", "WAV", Category::Audio);
    }
    if starts(b"RIFF") && at(8, b"AVI ") {
        return known("avi", "AVI", Category::Video);
    }
    if starts(b"ID3") || (head.len() >= 2 && head[0] == 0xFF && (head[1] & 0xE6) == 0xE2) {
        return known("mp3", "MP3", Category::Audio);
    }
    if starts(b"fLaC") {
        return known("flac", "FLAC", Category::Audio);
    }
    if starts(b"OggS") {
        return known("ogg", "Ogg", Category::Audio);
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return known("webm", "WebM / Matroska", Category::Video);
    }
    if at(4, b"ftyp") && head.len() >= 12 {
        return Some(match &head[8..12] {
            b"heic" | b"heix" | b"heim" | b"heis" | b"mif1" | b"msf1" => Known {
                id: "heic",
                label: "HEIC",
                category: Category::Image,
            },
            b"avif" | b"avis" => Known {
                id: "avif",
                label: "AVIF",
                category: Category::Image,
            },
            b"M4A " => Known {
                id: "m4a",
                label: "M4A",
                category: Category::Audio,
            },
            b"qt  " => Known {
                id: "mov",
                label: "QuickTime",
                category: Category::Video,
            },
            _ => Known {
                id: "mp4",
                label: "MP4",
                category: Category::Video,
            },
        });
    }
    if starts(b"PK\x03\x04") {
        return known("zip", "ZIP archive (or Office document)", Category::Archive);
    }
    if starts(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return known("ole", "Legacy Office document", Category::Document);
    }
    if starts(&[0x00, 0x00, 0x01, 0x00]) {
        return known("ico", "ICO", Category::Image);
    }
    if starts(b"8BPS") {
        return known("psd", "Photoshop PSD", Category::Image);
    }
    if starts(b"MZ") {
        return known("exe", "Windows executable", Category::Other);
    }
    let text = String::from_utf8_lossy(head)
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg")) {
        return known("svg", "SVG", Category::Image);
    }
    if text.starts_with("<!doctype html") || text.starts_with("<html") {
        return known("html", "HTML", Category::Document);
    }
    None
}

/// BMP has a weak two-byte signature; also require a known DIB header size.
fn looks_like_bmp(head: &[u8]) -> bool {
    if head.len() < 18 {
        return false;
    }
    let dib = u32::from_le_bytes([head[14], head[15], head[16], head[17]]);
    matches!(dib, 12 | 16 | 40 | 52 | 56 | 64 | 108 | 124)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// What the content was identified as (if anything).
    pub recognized: Option<Recognized>,
    /// Set when the file is a convertible format whose header decoded cleanly.
    pub format: Option<Format>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_alpha: bool,
    pub bit_depth: Option<u16>,
    /// The extension's format disagrees with the content.
    pub extension_mismatch: bool,
    /// When not readable, why (the UI shows this instead of the radial menu).
    pub problem: Option<RadialError>,
}

/// Inspect a file: size, content type and decoder-level validation.
///
/// Returns `Err` only for problems accessing the file itself (missing,
/// unreadable, a directory). Content problems are reported in
/// [`FileInfo::problem`] so the UI can still show the file's name and size.
pub fn inspect(path: &Path) -> Result<FileInfo> {
    let meta = std::fs::metadata(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    if !meta.is_file() {
        return Err(RadialError::new(
            ErrorKind::InvalidFile,
            "That is a folder, not a file.",
        ));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = FileInfo {
        path: path.to_path_buf(),
        name,
        size: meta.len(),
        recognized: None,
        format: None,
        width: None,
        height: None,
        has_alpha: false,
        bit_depth: None,
        extension_mismatch: false,
        problem: None,
    };

    if meta.len() == 0 {
        info.problem = Some(RadialError::new(
            ErrorKind::EmptyFile,
            "The file is empty (0 bytes).",
        ));
        return Ok(info);
    }
    if meta.len() > limits::MAX_SOURCE_BYTES {
        info.problem = Some(RadialError::new(
            ErrorKind::TooLarge,
            format!(
                "Files larger than {} MB are not supported.",
                limits::MAX_SOURCE_BYTES / 1_000_000
            ),
        ));
        return Ok(info);
    }

    let mut head = [0u8; SNIFF_LEN];
    let n = read_head(path, &mut head)?;
    info.recognized = sniff(&head[..n]);

    let ext_format = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension);

    match info.recognized {
        None => {
            info.problem = Some(RadialError::new(
                ErrorKind::InvalidFile,
                "RADIAL could not recognise this file's contents.",
            ));
        }
        Some(Recognized::Known { label, .. }) => {
            info.extension_mismatch = ext_format.is_some();
            info.problem = Some(RadialError::new(
                ErrorKind::UnsupportedFormat,
                format!("{label} files are recognised but cannot be converted yet."),
            ));
        }
        Some(Recognized::Convertible(format)) => {
            info.extension_mismatch = ext_format.is_some_and(|e| e != format);
            match read_header(path, format) {
                Ok(h) => {
                    info.format = Some(format);
                    info.width = Some(h.width);
                    info.height = Some(h.height);
                    info.has_alpha = h.has_alpha;
                    info.bit_depth = Some(h.bit_depth);
                }
                Err(e) => info.problem = Some(e),
            }
        }
    }
    Ok(info)
}

fn read_head(path: &Path, buf: &mut [u8]) -> Result<usize> {
    let mut f = File::open(path).map_err(|e| RadialError::from_io(&e, "opening the file"))?;
    let mut total = 0;
    while total < buf.len() {
        match f.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(RadialError::from_io(&e, "reading the file")),
        }
    }
    Ok(total)
}

pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub bit_depth: u16,
}

/// Parse just the header with the decoder for `format` (not guessed from the
/// extension), enforcing RADIAL's dimension limits.
pub(crate) fn read_header(path: &Path, format: Format) -> Result<Header> {
    let file = File::open(path).map_err(|e| RadialError::from_io(&e, "opening the file"))?;
    let mut reader = image::ImageReader::with_format(BufReader::new(file), format.image_format());
    reader.limits(limits::decode_limits());
    let decoder = reader
        .into_decoder()
        .map_err(|e| RadialError::from_image(e, "reading the header"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(RadialError::new(
            ErrorKind::CorruptFile,
            "The image has zero width or height.",
        ));
    }
    let color = decoder.color_type();
    Ok(Header {
        width,
        height,
        has_alpha: color.has_alpha(),
        bit_depth: color.bits_per_pixel() / u16::from(color.channel_count()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_image_signatures() {
        assert_eq!(
            sniff(b"\x89PNG\r\n\x1a\n...."),
            Some(Recognized::Convertible(Format::Png))
        );
        assert_eq!(
            sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(Recognized::Convertible(Format::Jpeg))
        );
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(Recognized::Convertible(Format::Webp))
        );
        assert_eq!(
            sniff(b"GIF89a..."),
            Some(Recognized::Convertible(Format::Gif))
        );
        assert_eq!(
            sniff(b"II*\0...."),
            Some(Recognized::Convertible(Format::Tiff))
        );
        assert_eq!(
            sniff(b"MM\0*...."),
            Some(Recognized::Convertible(Format::Tiff))
        );
        let mut bmp = vec![b'B', b'M'];
        bmp.extend_from_slice(&[0; 12]);
        bmp.extend_from_slice(&40u32.to_le_bytes());
        assert_eq!(sniff(&bmp), Some(Recognized::Convertible(Format::Bmp)));
    }

    #[test]
    fn rejects_weak_bmp_signature() {
        // Plain text beginning with "BM" must not be treated as a bitmap.
        assert_eq!(sniff(b"BMW owners manual, chapter one"), None);
    }

    #[test]
    fn recognises_unsupported_types() {
        let id = |b: &[u8]| match sniff(b) {
            Some(Recognized::Known { id, .. }) => id,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(id(b"%PDF-1.7"), "pdf");
        assert_eq!(id(b"RIFF\0\0\0\0WAVEfmt "), "wav");
        assert_eq!(id(b"\0\0\0\x18ftypheic"), "heic");
        assert_eq!(id(b"\0\0\0\x18ftypavif"), "avif");
        assert_eq!(id(b"\0\0\0\x18ftypisom"), "mp4");
        assert_eq!(id(b"PK\x03\x04"), "zip");
        assert_eq!(id(b"ID3\x04"), "mp3");
        assert_eq!(id(b"MZ\x90\0"), "exe");
        assert_eq!(id(b"<?xml version=\"1.0\"?><svg xmlns"), "svg");
    }

    #[test]
    fn unknown_content_is_none() {
        assert_eq!(sniff(b"hello world"), None);
        assert_eq!(sniff(b""), None);
    }
}
