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
    /// PDFium (bundled shared library) plus lopdf.
    Pdfium,
    /// lopdf, compiled into RADIAL: writes images into new PDFs.
    PdfWriter,
    /// Built-in reader for Word (.docx) packages.
    Docx,
}

impl EngineId {
    pub fn label(self) -> &'static str {
        match self {
            EngineId::NativeImage => "Built-in image engine",
            EngineId::Pdfium => "PDF engine (PDFium)",
            EngineId::PdfWriter => "Built-in PDF writer",
            EngineId::Docx => "Built-in Word reader",
        }
    }

    /// Built-in engines are always present. Engines loaded at runtime
    /// (PDFium) report availability from a probe.
    pub fn status(self) -> EngineStatus {
        match self {
            EngineId::NativeImage | EngineId::PdfWriter | EngineId::Docx => EngineStatus::Available,
            EngineId::Pdfium => match crate::pdf::missing_reason() {
                None => EngineStatus::Available,
                Some(hint) => EngineStatus::Missing { hint },
            },
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
    /// Choose a PDF page (PDF → image, and the editor's page).
    Page,
    /// Rendering resolution (PDF → image).
    Dpi {
        min: u32,
        max: u32,
        default: u32,
    },
    /// Apply to the current page or every page (Crop on PDFs).
    PageScope,
    /// Words or phrases to find and remove (Redact on PDFs).
    Terms,
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
        for from in Format::IMAGES {
            for to in Format::IMAGES {
                if from != to {
                    caps.push(image_capability(from, to));
                }
            }
            caps.push(image_to_pdf(from));
        }
        let pdfium = EngineId::Pdfium.status();
        for to in [Format::Png, Format::Jpeg, Format::Txt] {
            caps.push(pdf_capability(to, pdfium.clone()));
        }
        caps.push(document_capability(
            Format::Txt,
            Format::Pdf,
            EngineId::Pdfium,
            pdfium,
            vec![
                "Typeset on A4 in a single font and size; Markdown or other markup is not interpreted.",
                "Long lines are wrapped; tabs become spaces.",
            ],
        ));
        caps.push(document_capability(
            Format::Docx,
            Format::Txt,
            EngineId::Docx,
            EngineStatus::Available,
            vec![
                "Keeps the words and paragraphs of the main text; formatting, images, headers, footers, footnotes and comments are left out.",
                "Tracked deletions are left out; tracked insertions are included.",
            ],
        ));
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
        Format::Pdf | Format::Txt | Format::Docx => {}
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

fn image_to_pdf(from: Format) -> Capability {
    let engine = EngineId::PdfWriter;
    let mut limitations = vec![
        "The image is placed on an A4 page (portrait or landscape to match), centred, never enlarged past 96 DPI.",
    ];
    if from == Format::Jpeg {
        limitations.push("JPEG data is embedded unchanged unless the photo needs rotating.");
    } else {
        limitations.push("Pixels are stored losslessly; 16-bit images become 8-bit.");
    }
    if from == Format::Gif || from == Format::Webp {
        limitations.push("Only the first frame of an animation is used.");
    }
    Capability {
        from,
        to: Format::Pdf,
        to_label: Format::Pdf.label(),
        engine,
        engine_label: engine.label(),
        engine_status: EngineStatus::Available,
        available: true,
        options: Vec::new(),
        limitations,
        progress: ProgressKind::Stages,
        cancellation: CancelKind::BetweenStages,
    }
}

fn document_capability(
    from: Format,
    to: Format,
    engine: EngineId,
    status: EngineStatus,
    limitations: Vec<&'static str>,
) -> Capability {
    Capability {
        from,
        to,
        to_label: to.label(),
        engine,
        engine_label: engine.label(),
        available: status == EngineStatus::Available,
        engine_status: status,
        options: Vec::new(),
        limitations,
        progress: ProgressKind::Stages,
        cancellation: CancelKind::BetweenStages,
    }
}

fn pdf_capability(to: Format, status: EngineStatus) -> Capability {
    let engine = EngineId::Pdfium;
    let dpi = OptionSpec::Dpi {
        min: crate::pdf::MIN_DPI,
        max: crate::pdf::MAX_DPI,
        default: crate::pdf::DEFAULT_DPI,
    };
    let (options, limitations) = match to {
        Format::Txt => (
            Vec::new(),
            vec![
                "Extracts the text of every page in reading order; layout, tables and images are not kept.",
                "Scanned pages have no text layer and produce nothing (no OCR).",
            ],
        ),
        Format::Jpeg => (
            vec![
                OptionSpec::Page,
                dpi,
                OptionSpec::Quality {
                    min: 1,
                    max: 100,
                    default: crate::options::DEFAULT_JPEG_QUALITY,
                },
            ],
            vec!["Exports one page as a picture; its text is no longer selectable."],
        ),
        _ => (
            vec![OptionSpec::Page, dpi],
            vec!["Exports one page as a picture; its text is no longer selectable."],
        ),
    };
    Capability {
        from: Format::Pdf,
        to,
        to_label: to.label(),
        engine,
        engine_label: engine.label(),
        available: status == EngineStatus::Available,
        engine_status: status,
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
