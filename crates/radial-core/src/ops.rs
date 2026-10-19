//! Operations: the actions offered in the radial menu for a given file.
//!
//! [`operations_for`] is the single source of truth for what the UI may
//! offer. Every entry is backed by a handler in [`run`]; operations that are
//! recognised but can't be performed for a file are returned with
//! `available: false` and a reason, never silently hidden behind a broken
//! button.

use crate::detect::{self, FileInfo};
use crate::engine::{self, Stage};
use crate::error::{ErrorKind, RadialError, Result};
use crate::format::Format;
use crate::metadata::{self, MetadataEdit};
use crate::options::{ConvertOptions, Resize};
use crate::output;
use crate::paths;
use crate::registry::{CancelKind, Capability, OptionSpec, Registry};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationId {
    #[default]
    Convert,
    Compress,
    Edit,
    Crop,
    Annotate,
    Redact,
    Background,
    Metadata,
}

impl OperationId {
    /// Display order in the ring, clockwise from 12 o'clock.
    pub const ALL: [OperationId; 8] = [
        OperationId::Convert,
        OperationId::Compress,
        OperationId::Edit,
        OperationId::Crop,
        OperationId::Annotate,
        OperationId::Redact,
        OperationId::Background,
        OperationId::Metadata,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OperationId::Convert => "Convert",
            OperationId::Compress => "Compress",
            OperationId::Edit => "Edit",
            OperationId::Crop => "Crop",
            OperationId::Annotate => "Annotate",
            OperationId::Redact => "Redact",
            OperationId::Background => "Background",
            OperationId::Metadata => "Metadata",
        }
    }

    /// Suffix for the default output name, so results never look like the original.
    pub fn stem_suffix(self) -> &'static str {
        match self {
            OperationId::Convert => "",
            OperationId::Compress => "-compressed",
            OperationId::Edit => "-edited",
            OperationId::Crop => "-cropped",
            OperationId::Annotate => "-annotated",
            OperationId::Redact => "-redacted",
            OperationId::Background => "-background",
            OperationId::Metadata => "-metadata",
        }
    }

    /// Operations whose parameters contain user content (text, positions of
    /// sensitive areas, metadata values). Their parameters are never stored
    /// in history.
    pub fn is_sensitive(self) -> bool {
        matches!(
            self,
            OperationId::Annotate | OperationId::Redact | OperationId::Metadata
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: OperationId,
    pub label: &'static str,
    pub description: String,
    pub available: bool,
    /// Why the operation can't run for this file (when `available` is false).
    pub reason: Option<String>,
    pub engine_label: &'static str,
    pub options: Vec<OptionSpec>,
    pub limitations: Vec<String>,
    /// Format of the result; `None` when the user chooses (Convert).
    pub output_format: Option<Format>,
    /// Convert targets, from the conversion registry.
    pub targets: Vec<Capability>,
    /// The UI can show a live preview of the result before running.
    pub preview: bool,
    pub cancellation: CancelKind,
}

const IMAGE_ENGINE: &str = "Built-in image engine";

fn op(id: OperationId, description: impl Into<String>, output: Option<Format>) -> Operation {
    Operation {
        id,
        label: id.label(),
        description: description.into(),
        available: true,
        reason: None,
        engine_label: IMAGE_ENGINE,
        options: Vec::new(),
        limitations: Vec::new(),
        output_format: output,
        targets: Vec::new(),
        preview: false,
        cancellation: CancelKind::BetweenStages,
    }
}

fn unavailable(mut o: Operation, reason: impl Into<String>) -> Operation {
    o.available = false;
    o.reason = Some(reason.into());
    o
}

/// Operations for a detected, readable file.
pub fn operations_for(registry: &Registry, info: &FileInfo) -> Vec<Operation> {
    let Some(f) = info.format.filter(|_| info.problem.is_none()) else {
        return Vec::new();
    };
    let quality = |o: &mut Operation| o.options.extend(quality_spec(f));
    let gif_note =
        (f == Format::Gif).then(|| "Only the first frame of an animated GIF is kept.".to_string());
    let reencode_note = match f {
        Format::Jpeg => Some(
            "The image is re-encoded as JPEG, which loses a little detail each time.".to_string(),
        ),
        Format::Webp => Some("The image is re-encoded as WebP.".to_string()),
        _ => None,
    };
    let common = |o: &mut Operation| {
        o.limitations.extend(gif_note.clone());
        if !matches!(o.id, OperationId::Metadata) {
            o.limitations
                .push("Camera metadata (EXIF, GPS) is not copied to the result.".into());
        }
    };

    let mut ops = Vec::new();

    let mut convert = op(OperationId::Convert, "Save in another format", None);
    convert.targets = registry.targets_for(f);
    convert.available = convert.targets.iter().any(|c| c.available);
    ops.push(convert);

    let mut compress = op(OperationId::Compress, "Make the file smaller", Some(f));
    match f {
        Format::Jpeg | Format::Webp => {
            quality(&mut compress);
            compress.options.push(OptionSpec::MaxDimension);
            compress.limitations.extend(reencode_note.clone());
        }
        Format::Png => {
            compress
                .options
                .extend([OptionSpec::Optimize, OptionSpec::MaxDimension]);
            compress
                .limitations
                .push("Lossless: pixels are unchanged unless you also reduce the size.".into());
        }
        Format::Tiff => {
            compress.options.push(OptionSpec::MaxDimension);
            compress
                .limitations
                .push("Saved with lossless LZW compression.".into());
        }
        Format::Bmp => {
            compress = unavailable(
                compress,
                "BMP is uncompressed by design. Convert to PNG for a lossless smaller file.",
            );
        }
        Format::Gif => {
            compress = unavailable(
                compress,
                "GIF compression would drop animation frames. Convert to PNG or WebP instead.",
            );
        }
    }
    compress
        .limitations
        .push("If the result isn't smaller than the original, nothing is saved.".into());
    common(&mut compress);
    ops.push(compress);

    let mut edit = op(OperationId::Edit, "Rotate, resize and adjust tone", Some(f));
    edit.options.extend([
        OptionSpec::Rotate,
        OptionSpec::Resize,
        OptionSpec::BrightnessContrast,
    ]);
    quality(&mut edit);
    edit.preview = true;
    edit.limitations.extend(reencode_note.clone());
    common(&mut edit);
    ops.push(edit);

    let mut crop = op(OperationId::Crop, "Trim to an area", Some(f));
    crop.options.extend([OptionSpec::Crop, OptionSpec::Rotate]);
    crop.preview = true;
    crop.limitations.extend(reencode_note.clone());
    common(&mut crop);
    ops.push(crop);

    let mut annotate = op(
        OperationId::Annotate,
        "Draw arrows, shapes and text",
        Some(f),
    );
    annotate.options.push(OptionSpec::Shapes);
    annotate.preview = true;
    annotate
        .limitations
        .push("Annotations become part of the image pixels in the saved copy.".into());
    annotate.limitations.push("Text uses DejaVu Sans; right-to-left and complex scripts (e.g. Arabic) are not shaped correctly yet.".into());
    if info.bit_depth == Some(16) {
        annotate
            .limitations
            .push("16-bit images are saved as 8-bit when annotated.".into());
    }
    common(&mut annotate);
    ops.push(annotate);

    let mut redact = op(OperationId::Redact, "Black out sensitive areas", Some(f));
    redact.options.push(OptionSpec::Regions);
    redact.preview = true;
    redact.limitations.push("The pixels under each box are replaced with solid black in the saved copy; nothing underneath can be recovered from it.".into());
    redact
        .limitations
        .push("All metadata, including location and embedded thumbnails, is removed.".into());
    redact.limitations.extend(gif_note.clone());
    ops.push(redact);

    let mut background = op(
        OperationId::Background,
        if info.has_alpha {
            "Fill transparency with a colour"
        } else {
            "Add a coloured border"
        },
        Some(f),
    );
    background
        .options
        .extend([OptionSpec::Background, OptionSpec::Padding]);
    background.preview = true;
    if !info.has_alpha {
        background.limitations.push(
            "This image has no transparency, so only the border (padding) changes it.".into(),
        );
    }
    common(&mut background);
    ops.push(background);

    let mut meta = op(
        OperationId::Metadata,
        "View, edit or remove metadata",
        Some(f),
    );
    if metadata::supports_editing(f) {
        meta.options
            .extend([OptionSpec::MetadataFields, OptionSpec::StripMetadata]);
        meta.limitations
            .push("Edits are lossless: image data is copied unchanged.".into());
    } else {
        meta.limitations.push(format!(
            "{} metadata can be viewed but not edited yet.",
            f.label()
        ));
    }
    ops.push(meta);

    ops
}

fn quality_spec(f: Format) -> Option<OptionSpec> {
    match f {
        Format::Jpeg => Some(OptionSpec::Quality {
            min: 1,
            max: 100,
            default: 75,
        }),
        Format::Webp => Some(OptionSpec::Quality {
            min: 1,
            max: 100,
            default: 75,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRequest {
    pub source: PathBuf,
    pub operation: OperationId,
    /// Convert only.
    #[serde(default)]
    pub target: Option<Format>,
    pub dest_dir: PathBuf,
    pub stem: String,
    #[serde(default)]
    pub options: ConvertOptions,
    #[serde(default)]
    pub metadata: MetadataEdit,
    /// Compress: longest side in pixels.
    #[serde(default)]
    pub max_dimension: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationOutcome {
    pub output_path: PathBuf,
    pub operation: OperationId,
    pub source_format: Format,
    pub format: Format,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
    pub source_size_bytes: u64,
    pub notes: Vec<String>,
}

fn invalid(msg: &str) -> RadialError {
    RadialError::new(ErrorKind::InvalidOptions, msg)
}

/// Keep only the options an operation is allowed to change, so a request
/// can't smuggle unrelated edits into, say, a redaction.
fn options_for(
    op: OperationId,
    info: &FileInfo,
    req: &OperationRequest,
) -> Result<(ConvertOptions, Option<u64>)> {
    let o = &req.options;
    let mut clean = ConvertOptions {
        background: o.background.clone(),
        ..Default::default()
    };
    let mut shrink_below = None;
    match op {
        OperationId::Convert => return Ok((o.clone(), None)),
        OperationId::Compress => {
            clean.quality = o.quality;
            clean.optimize = info.format == Some(Format::Png);
            if let Some(m) = req.max_dimension {
                let (w, h) = (info.width.unwrap_or(0), info.height.unwrap_or(0));
                if m == 0 {
                    return Err(invalid("The maximum size must be at least 1 pixel."));
                }
                if w > m || h > m {
                    clean.resize = Some(Resize {
                        width: Some(m),
                        height: Some(m),
                        keep_aspect: true,
                    });
                }
            }
            shrink_below = Some(info.size);
        }
        OperationId::Edit => {
            clean.quality = o.quality;
            clean.lossless = o.lossless;
            clean.resize = o.resize;
            clean.edits = o.edits;
            clean.edits.crop = None;
            if clean.edits.is_identity() && clean.resize.is_none() {
                return Err(invalid(
                    "Nothing to change: rotate, resize or adjust the image first.",
                ));
            }
        }
        OperationId::Crop => {
            clean.edits.rotate = o.edits.rotate;
            clean.edits.crop = o.edits.crop;
            if clean.edits.crop.is_none() {
                return Err(invalid("Draw the area to keep first."));
            }
        }
        OperationId::Annotate => {
            clean.annotations = o.annotations.clone();
            if clean.annotations.is_empty() {
                return Err(invalid("Add at least one annotation first."));
            }
        }
        OperationId::Redact => {
            clean.redactions = o.redactions.clone();
            if clean.redactions.is_empty() {
                return Err(invalid("Mark at least one area to redact first."));
            }
        }
        OperationId::Background => {
            clean.flatten = true;
            clean.padding = o.padding;
            if !info.has_alpha && o.padding == 0 {
                return Err(invalid(
                    "This image has no transparency. Add a border to use Background.",
                ));
            }
        }
        OperationId::Metadata => unreachable!("handled separately"),
    }
    Ok((clean, shrink_below))
}

/// Default output name (without extension) for an operation on `source`.
pub fn default_stem(op: OperationId, source: &Path) -> String {
    let base = crate::naming::default_stem(source);
    let suffix = op.stem_suffix();
    let max = 200usize.saturating_sub(suffix.chars().count());
    format!("{}{}", base.chars().take(max).collect::<String>(), suffix)
}

/// Validate and execute an operation. `on_stage` is called as each stage begins.
pub fn run(
    registry: &Registry,
    req: &OperationRequest,
    cancel: &AtomicBool,
    mut on_stage: impl FnMut(Stage),
) -> Result<OperationOutcome> {
    let source = paths::validate_source(&req.source)?;
    let info = detect::inspect(&source)?;
    if let Some(p) = info.problem.clone() {
        return Err(p);
    }
    let from = info
        .format
        .ok_or_else(|| RadialError::new(ErrorKind::UnsupportedFormat, "Unsupported file."))?;
    let ops = operations_for(registry, &info);
    let op = ops.iter().find(|o| o.id == req.operation).ok_or_else(|| {
        RadialError::new(
            ErrorKind::UnsupportedConversion,
            format!(
                "{} isn't available for {} files.",
                req.operation.label(),
                from.label()
            ),
        )
    })?;
    if !op.available {
        return Err(RadialError::new(
            ErrorKind::UnsupportedConversion,
            op.reason
                .clone()
                .unwrap_or_else(|| "This operation isn't available.".into()),
        ));
    }

    if req.operation == OperationId::Metadata {
        return run_metadata(&source, from, &info, req, cancel, on_stage);
    }

    let target = match req.operation {
        OperationId::Convert => {
            let t = req
                .target
                .ok_or_else(|| invalid("Choose a format to convert to."))?;
            if !op.targets.iter().any(|c| c.to == t && c.available) {
                return Err(RadialError::new(
                    ErrorKind::UnsupportedConversion,
                    format!(
                        "{} → {} is not a supported conversion.",
                        from.label(),
                        t.label()
                    ),
                ));
            }
            t
        }
        _ => from,
    };
    let (options, shrink_below) = options_for(req.operation, &info, req)?;
    let resolved = options.validate(target)?;
    let out = output::plan(&source, &req.dest_dir, &req.stem, target.extension())?;
    let r = engine::image_job(
        &source,
        from,
        target,
        &resolved,
        &out,
        shrink_below,
        cancel,
        &mut on_stage,
    )?;

    let mut notes = Vec::new();
    if req.operation == OperationId::Redact {
        let n = resolved.redactions.len();
        notes.push(format!(
            "{n} area{} replaced with black pixels; metadata removed.",
            if n == 1 { "" } else { "s" }
        ));
    }
    Ok(OperationOutcome {
        output_path: out,
        operation: req.operation,
        source_format: from,
        format: target,
        width: Some(r.width),
        height: Some(r.height),
        size_bytes: r.size_bytes,
        source_size_bytes: info.size,
        notes,
    })
}

fn run_metadata(
    source: &Path,
    from: Format,
    info: &FileInfo,
    req: &OperationRequest,
    cancel: &AtomicBool,
    mut on_stage: impl FnMut(Stage),
) -> Result<OperationOutcome> {
    if !metadata::supports_editing(from) {
        return Err(RadialError::new(
            ErrorKind::UnsupportedFormat,
            format!("Editing metadata isn't supported for {} yet.", from.label()),
        ));
    }
    req.metadata.validate(&metadata::editable_keys(from))?;
    let out = output::plan(source, &req.dest_dir, &req.stem, from.extension())?;
    engine::check_cancel(cancel)?;
    on_stage(Stage::Decoding);
    let data = std::fs::read(source).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    on_stage(Stage::Processing);
    let (bytes, notes) = metadata::write_image(&data, from, &req.metadata)?;
    engine::check_cancel(cancel)?;
    on_stage(Stage::Encoding);
    let mut tmp = output::stage(&out)?;
    tmp.write_all(&bytes)
        .map_err(|e| RadialError::from_io(&e, "writing the output"))?;
    output::sync(&tmp)?;
    engine::check_cancel(cancel)?;
    on_stage(Stage::Verifying);
    // The image itself must still be the same format and size, decode fully,
    // and carry the new values.
    let check = detect::inspect(tmp.path())?;
    if check.problem.is_some()
        || check.format != Some(from)
        || (check.width, check.height) != (info.width, info.height)
    {
        return Err(RadialError::new(
            ErrorKind::EngineFailed,
            "The edited file failed verification.",
        ));
    }
    engine::load(tmp.path(), from).map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("The edited file could not be read back: {}", e.message),
        )
    })?;
    metadata::verify_image(tmp.path(), from, &req.metadata)?;
    engine::check_cancel(cancel)?;
    let size_bytes = output::persist(tmp, &out)?;
    Ok(OperationOutcome {
        output_path: out,
        operation: OperationId::Metadata,
        source_format: from,
        format: from,
        width: info.width,
        height: info.height,
        size_bytes,
        source_size_bytes: info.size,
        notes,
    })
}
