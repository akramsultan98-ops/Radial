//! File formats RADIAL knows about.
//!
//! [`Format`] lists formats with a conversion engine. [`Recognized`] covers
//! everything RADIAL can identify by content, including types it cannot
//! convert yet, so the UI can explain *why* a file is rejected.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Gif,
    Pdf,
    /// Plain UTF-8 text (output of text extraction).
    Txt,
}

impl Format {
    /// Raster image formats handled by the image engine.
    pub const IMAGES: [Format; 6] = [
        Format::Png,
        Format::Jpeg,
        Format::Webp,
        Format::Bmp,
        Format::Tiff,
        Format::Gif,
    ];

    pub const ALL: [Format; 8] = [
        Format::Png,
        Format::Jpeg,
        Format::Webp,
        Format::Bmp,
        Format::Tiff,
        Format::Gif,
        Format::Pdf,
        Format::Txt,
    ];

    pub fn is_image(self) -> bool {
        Format::IMAGES.contains(&self)
    }

    pub fn id(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpeg",
            Format::Webp => "webp",
            Format::Bmp => "bmp",
            Format::Tiff => "tiff",
            Format::Gif => "gif",
            Format::Pdf => "pdf",
            Format::Txt => "txt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Bmp => "BMP",
            Format::Tiff => "TIFF",
            Format::Gif => "GIF",
            Format::Pdf => "PDF",
            Format::Txt => "Text",
        }
    }

    /// The extension written for new files of this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Bmp => "bmp",
            Format::Tiff => "tiff",
            Format::Gif => "gif",
            Format::Pdf => "pdf",
            Format::Txt => "txt",
        }
    }

    /// All extensions conventionally used for this format (lowercase).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Png => &["png"],
            Format::Jpeg => &["jpg", "jpeg", "jpe", "jfif"],
            Format::Webp => &["webp"],
            Format::Bmp => &["bmp", "dib"],
            Format::Tiff => &["tif", "tiff"],
            Format::Gif => &["gif"],
            Format::Pdf => &["pdf"],
            Format::Txt => &["txt", "text"],
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
            Format::Bmp => "image/bmp",
            Format::Tiff => "image/tiff",
            Format::Gif => "image/gif",
            Format::Pdf => "application/pdf",
            Format::Txt => "text/plain",
        }
    }

    /// Whether RADIAL writes an alpha channel for this format. Formats without
    /// alpha are flattened onto a background colour.
    pub fn writes_alpha(self) -> bool {
        matches!(
            self,
            Format::Png | Format::Webp | Format::Tiff | Format::Gif
        )
    }

    /// Whether the encoder exposes a lossy quality setting.
    pub fn has_quality(self) -> bool {
        matches!(self, Format::Jpeg | Format::Webp)
    }

    /// Normalise user input such as `"JPG"`, `".jpeg"` or `"image/png"`.
    pub fn parse(input: &str) -> Option<Format> {
        let s = input.trim().trim_start_matches('.').to_ascii_lowercase();
        Format::ALL
            .into_iter()
            .find(|f| f.id() == s || f.mime() == s || f.extensions().contains(&s.as_str()))
    }

    pub fn from_extension(ext: &str) -> Option<Format> {
        let e = ext.to_ascii_lowercase();
        Format::ALL
            .into_iter()
            .find(|f| f.extensions().contains(&e.as_str()))
    }

    /// The `image` crate codec, for raster formats only.
    pub(crate) fn image_format(self) -> Option<image::ImageFormat> {
        Some(match self {
            Format::Png => image::ImageFormat::Png,
            Format::Jpeg => image::ImageFormat::Jpeg,
            Format::Webp => image::ImageFormat::WebP,
            Format::Bmp => image::ImageFormat::Bmp,
            Format::Tiff => image::ImageFormat::Tiff,
            Format::Gif => image::ImageFormat::Gif,
            Format::Pdf | Format::Txt => return None,
        })
    }

    /// The `image` crate codec; errors for non-raster formats.
    pub(crate) fn codec(self) -> crate::Result<image::ImageFormat> {
        self.image_format().ok_or_else(|| {
            crate::RadialError::new(
                crate::ErrorKind::Internal,
                format!("{} is not an image format.", self.label()),
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Document,
    Audio,
    Video,
    Archive,
    Other,
}

/// What content sniffing identified, independent of whether we can convert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "format", rename_all = "snake_case")]
pub enum Recognized {
    Convertible(Format),
    /// Identified, but no conversion engine handles it yet.
    Known {
        id: &'static str,
        label: &'static str,
        category: Category,
    },
}

impl Recognized {
    pub fn label(&self) -> &'static str {
        match self {
            Recognized::Convertible(f) => f.label(),
            Recognized::Known { label, .. } => label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aliases() {
        assert_eq!(Format::parse("JPG"), Some(Format::Jpeg));
        assert_eq!(Format::parse(".jpeg"), Some(Format::Jpeg));
        assert_eq!(Format::parse("image/webp"), Some(Format::Webp));
        assert_eq!(Format::parse("tif"), Some(Format::Tiff));
        assert_eq!(Format::parse(" png "), Some(Format::Png));
        assert_eq!(Format::parse("heic"), None);
    }

    #[test]
    fn ids_round_trip_through_serde() {
        for f in Format::ALL {
            let json = serde_json::to_string(&f).unwrap();
            assert_eq!(json, format!("\"{}\"", f.id()));
            let back: Format = serde_json::from_str(&json).unwrap();
            assert_eq!(back, f);
        }
    }
}
