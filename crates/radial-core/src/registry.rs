//! The conversion capability registry: the single source of truth for which
//! conversions exist, which engine performs them, and what they support.
//!
//! The UI never keeps its own list of formats. It asks the registry for the
//! targets of a detected source format, and every entry returned here is backed
//! by a real handler in [`crate::engine`].

use crate::format::Format;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineId {
    /// Pure-Rust `image` crate plus libwebp, compiled into RADIAL.
    NativeImage,
}

impl EngineId {
    pub fn label(self) -> &'static str {
        match self {
            EngineId::NativeImage => "Built-in image engine",
        }
    }

    /// Built-in engines are always present. External engines (FFmpeg,
    /// LibreOffice) will report availability from a runtime probe.
    pub fn status(self) -> EngineStatus {
        match self {
            EngineId::NativeImage => EngineStatus::Available,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineStatus {
    Available,
    Missing { hint: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptionSpec {
    Quality {
        min: u8,
        max: u8,
        default: u8,
    },
    Lossless,
    /// Background colour for flattening transparency.
    Background,
    Resize,
    Rotate,
    Crop,
    BrightnessContrast,
    /// Fit within a maximum width/height (Compress).
    MaxDimension,
    /// Border around the image (Background).
    Padding,
    /// Lossless optimisation (Compress, PNG).
    Optimize,
    /// Drawn shapes, arrows and text (Annotate).
    Shapes,
    /// Areas to remove (Redact).
    Regions,
    /// Title/author/… (Metadata).
    MetadataFields,
    /// Remove all metadata (Metadata).
    StripMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    /// Reports named stages (decode → process → encode → verify), not percentages.
    Stages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelKind {
    /// Cancellation is honoured between stages; nothing is written on cancel.
    BetweenStages,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub from: Format,
    pub to: Format,
    pub to_label: &'static str,
    pub engine: EngineId,
    pub engine_label: &'static str,
    pub engine_status: EngineStatus,
    /// True only when the engine is available and a handler exists.
    pub available: bool,
    pub options: Vec<OptionSpec>,
    pub limitations: Vec<&'static str>,
    pub progress: ProgressKind,
    pub cancellation: CancelKind,
}

#[derive(Debug, Clone)]
pub struct Registry {
    caps: Vec<Capability>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut caps = Vec::new();
        for from in Format::ALL {
            for to in Format::ALL {
                if from != to {
                    caps.push(image_capability(from, to));
                }
            }
        }
        Self { caps }
    }

    pub fn all(&self) -> &[Capability] {
        &self.caps
    }

    /// Conversions offered for a source format, in a stable display order.
    pub fn targets_for(&self, from: Format) -> Vec<Capability> {
        self.caps
            .iter()
            .filter(|c| c.from == from)
            .cloned()
            .collect()
    }

    pub fn find(&self, from: Format, to: Format) -> Option<&Capability> {
        self.caps.iter().find(|c| c.from == from && c.to == to)
    }

    pub fn sources(&self) -> Vec<Format> {
        let mut v: Vec<Format> = self.caps.iter().map(|c| c.from).collect();
        v.sort();
        v.dedup();
        v
    }
}

fn image_capability(from: Format, to: Format) -> Capability {
    let mut options = Vec::new();
    match to {
        Format::Jpeg => options.push(OptionSpec::Quality {
            min: 1,
            max: 100,
            default: crate::options::DEFAULT_JPEG_QUALITY,
        }),
        Format::Webp => {
            options.push(OptionSpec::Quality {
                min: 1,
                max: 100,
                default: crate::options::DEFAULT_WEBP_QUALITY,
            });
            options.push(OptionSpec::Lossless);
        }
        _ => {}
    }
    if !to.writes_alpha() {
        options.push(OptionSpec::Background);
    }
    options.extend([
        OptionSpec::Resize,
        OptionSpec::Rotate,
        OptionSpec::Crop,
        OptionSpec::BrightnessContrast,
    ]);

    let mut limitations =
        vec!["Camera metadata (EXIF, GPS) is not copied; orientation is applied to the pixels."];
    match from {
        Format::Gif => limitations.push("Only the first frame of an animated GIF is converted."),
        Format::Webp => limitations.push("Only the first frame of an animated WebP is converted."),
        _ => {}
    }
    if !to.writes_alpha() {
        limitations.push("Transparent areas are filled with the background colour.");
    }
    match to {
        Format::Jpeg => limitations
            .push("Lossy: lower quality discards more detail. 16-bit images become 8-bit."),
        Format::Webp => limitations.push(
            "16-bit images become 8-bit. Embedded colour profiles are not carried into WebP.",
        ),
        Format::Gif => limitations.push(
            "Reduced to a 256-colour palette; partial transparency becomes fully clear or opaque.",
        ),
        Format::Bmp => {
            limitations.push("16-bit images become 8-bit. Colour profiles are not stored.")
        }
        Format::Tiff => limitations
            .push("LZW-compressed (lossless). Colour profiles are not stored in TIFF output."),
        Format::Png => {}
    }

    let engine = EngineId::NativeImage;
    let engine_status = engine.status();
    Capability {
        from,
        to,
        to_label: to.label(),
        engine,
        engine_label: engine.label(),
        available: engine_status == EngineStatus::Available,
        engine_status,
        options,
        limitations,
        progress: ProgressKind::Stages,
        cancellation: CancelKind::BetweenStages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_required_phase_one_conversions() {
        let r = Registry::new();
        let required = [
            (Format::Png, Format::Jpeg),
            (Format::Jpeg, Format::Png),
            (Format::Png, Format::Webp),
            (Format::Webp, Format::Png),
            (Format::Jpeg, Format::Webp),
            (Format::Webp, Format::Jpeg),
            (Format::Bmp, Format::Png),
            (Format::Tiff, Format::Png),
        ];
        for (from, to) in required {
            let c = r
                .find(from, to)
                .unwrap_or_else(|| panic!("{from:?}->{to:?} missing"));
            assert!(c.available);
        }
    }

    #[test]
    fn never_offers_identity_conversion() {
        let r = Registry::new();
        assert!(r.all().iter().all(|c| c.from != c.to));
        assert!(r
            .targets_for(Format::Png)
            .iter()
            .all(|c| c.to != Format::Png));
    }

    #[test]
    fn options_match_target_format() {
        let r = Registry::new();
        let jpg = r.find(Format::Png, Format::Jpeg).unwrap();
        assert!(jpg.options.contains(&OptionSpec::Background));
        assert!(jpg
            .options
            .iter()
            .any(|o| matches!(o, OptionSpec::Quality { .. })));
        let png = r.find(Format::Jpeg, Format::Png).unwrap();
        assert!(!png.options.contains(&OptionSpec::Background));
        assert!(!png
            .options
            .iter()
            .any(|o| matches!(o, OptionSpec::Quality { .. })));
        let webp = r.find(Format::Png, Format::Webp).unwrap();
        assert!(webp.options.contains(&OptionSpec::Lossless));
    }

    #[test]
    fn gif_source_documents_first_frame_limitation() {
        let r = Registry::new();
        let c = r.find(Format::Gif, Format::Png).unwrap();
        assert!(c.limitations.iter().any(|l| l.contains("first frame")));
    }
}
