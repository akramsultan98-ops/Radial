//! The built-in image conversion engine.
//!
//! Pipeline: decode (with EXIF orientation applied) → rotate → crop → resize →
//! brightness/contrast → prepare pixels for the target → encode into a hidden
//! temporary file beside the destination → verify by decoding it again →
//! atomically move it into place without overwriting anything.
//!
//! Cancellation is cooperative and checked between stages. A cancelled or
//! failed job never leaves a file that looks like a finished conversion.

use crate::detect;
use crate::draw;
use crate::error::{ErrorKind, RadialError, Result};
use crate::format::{Format, Recognized};
use crate::limits;
use crate::options::{self, ConvertOptions, Edits, Resolved, Rgb};
use crate::output;
use crate::paths;
use crate::registry::Registry;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageEncoder};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Decoding,
    Processing,
    Encoding,
    Verifying,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub source: PathBuf,
    pub target: Format,
    pub dest_dir: PathBuf,
    /// Output file name without extension.
    pub stem: String,
    #[serde(default)]
    pub options: ConvertOptions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOutcome {
    pub output_path: PathBuf,
    pub source_format: Format,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

/// A decoded image plus the colour profile it carried.
pub struct Loaded {
    pub image: DynamicImage,
    pub icc: Option<Vec<u8>>,
}

/// Decode `path` with the decoder for `format`, applying EXIF orientation.
pub fn load(path: &Path, format: Format) -> Result<Loaded> {
    let file = File::open(path).map_err(|e| RadialError::from_io(&e, "opening the source file"))?;
    let mut reader = image::ImageReader::with_format(BufReader::new(file), format.codec()?);
    reader.limits(limits::decode_limits());
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| RadialError::from_image(e, "decoding the image"))?;
    let icc = decoder
        .icc_profile()
        .ok()
        .flatten()
        .filter(|p| !p.is_empty());
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| RadialError::from_image(e, "decoding the image"))?;
    image.apply_orientation(orientation);
    Ok(Loaded { image, icc })
}

/// Apply rotation, crop and tonal edits (but not resize).
pub fn apply_edits(mut img: DynamicImage, edits: &Edits) -> DynamicImage {
    img = match edits.rotate {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img,
    };
    if let Some(c) = edits.crop {
        let (w, h) = (f64::from(img.width()), f64::from(img.height()));
        let x = (c.x * w).round().clamp(0.0, w - 1.0) as u32;
        let y = (c.y * h).round().clamp(0.0, h - 1.0) as u32;
        let cw = ((c.width * w).round() as u32).clamp(1, img.width() - x);
        let ch = ((c.height * h).round() as u32).clamp(1, img.height() - y);
        if (x, y, cw, ch) != (0, 0, img.width(), img.height()) {
            img = img.crop_imm(x, y, cw, ch);
        }
    }
    adjust_tone(&mut img, edits.brightness, edits.contrast);
    img
}

/// Brightness/contrast via lookup tables, preserving bit depth and leaving
/// the alpha channel untouched.
fn adjust_tone(img: &mut DynamicImage, brightness: i8, contrast: i8) {
    if brightness == 0 && contrast == 0 {
        return;
    }
    let offset = f32::from(brightness) / 100.0 * 0.5;
    let factor = 1.0 + f32::from(contrast) / 100.0;
    let map = |v: f32| ((v - 0.5) * factor + 0.5 + offset).clamp(0.0, 1.0);

    fn apply<P: image::Pixel>(
        buf: &mut image::ImageBuffer<P, Vec<P::Subpixel>>,
        color_channels: usize,
        f: impl Fn(P::Subpixel) -> P::Subpixel,
    ) {
        let n = usize::from(P::CHANNEL_COUNT);
        for px in buf.chunks_exact_mut(n) {
            for c in &mut px[..color_channels] {
                *c = f(*c);
            }
        }
    }
    let lut8: Vec<u8> = (0..=255u16)
        .map(|v| (map(f32::from(v) / 255.0) * 255.0).round() as u8)
        .collect();
    let lut16 = || -> Vec<u16> {
        (0..=65535u32)
            .map(|v| (map(v as f32 / 65535.0) * 65535.0).round() as u16)
            .collect()
    };
    match img {
        DynamicImage::ImageLuma8(b) => apply(b, 1, |v| lut8[usize::from(v)]),
        DynamicImage::ImageLumaA8(b) => apply(b, 1, |v| lut8[usize::from(v)]),
        DynamicImage::ImageRgb8(b) => apply(b, 3, |v| lut8[usize::from(v)]),
        DynamicImage::ImageRgba8(b) => apply(b, 3, |v| lut8[usize::from(v)]),
        DynamicImage::ImageLuma16(b) => {
            let l = lut16();
            apply(b, 1, |v| l[usize::from(v)])
        }
        DynamicImage::ImageLumaA16(b) => {
            let l = lut16();
            apply(b, 1, |v| l[usize::from(v)])
        }
        DynamicImage::ImageRgb16(b) => {
            let l = lut16();
            apply(b, 3, |v| l[usize::from(v)])
        }
        DynamicImage::ImageRgba16(b) => {
            let l = lut16();
            apply(b, 3, |v| l[usize::from(v)])
        }
        DynamicImage::ImageRgb32F(b) => apply(b, 3, map),
        DynamicImage::ImageRgba32F(b) => apply(b, 3, map),
        other => {
            let mut rgba = other.to_rgba32f();
            apply(&mut rgba, 3, map);
            *other = DynamicImage::ImageRgba32F(rgba);
        }
    }
}

fn resize(img: DynamicImage, r: &options::Resize) -> DynamicImage {
    let (w, h) = options::resized_dimensions(img.width(), img.height(), r);
    if (w, h) == (img.width(), img.height()) {
        return img;
    }
    img.resize_exact(w, h, FilterType::Lanczos3)
}

/// Composite onto an opaque background colour.
fn flatten(img: &DynamicImage, bg: Rgb) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (src, dst) in rgba.pixels().zip(out.pixels_mut()) {
        let a = u32::from(src[3]);
        for i in 0..3 {
            let v = (u32::from(src[i]) * a + u32::from(bg.0[i]) * (255 - a) + 127) / 255;
            dst[i] = v as u8;
        }
    }
    DynamicImage::ImageRgb8(out)
}

/// Convert pixels to a layout the target encoder accepts.
fn prepare(img: DynamicImage, target: Format, bg: Rgb) -> DynamicImage {
    use DynamicImage as D;
    let has_alpha = img.color().has_alpha();
    match target {
        Format::Jpeg | Format::Bmp => {
            if has_alpha {
                flatten(&img, bg)
            } else if matches!(img, D::ImageLuma8(_) | D::ImageLuma16(_)) {
                D::ImageLuma8(img.to_luma8())
            } else if matches!(img, D::ImageRgb8(_)) {
                img
            } else {
                D::ImageRgb8(img.to_rgb8())
            }
        }
        Format::Webp => {
            if has_alpha {
                D::ImageRgba8(img.to_rgba8())
            } else if matches!(img, D::ImageRgb8(_)) {
                img
            } else {
                D::ImageRgb8(img.to_rgb8())
            }
        }
        Format::Gif => D::ImageRgba8(img.to_rgba8()),
        Format::Png => match img {
            D::ImageRgb32F(_) => D::ImageRgb16(img.to_rgb16()),
            D::ImageRgba32F(_) => D::ImageRgba16(img.to_rgba16()),
            other => other,
        },
        Format::Pdf | Format::Txt | Format::Docx => img,
        Format::Tiff => match img {
            D::ImageLuma8(_)
            | D::ImageLuma16(_)
            | D::ImageRgb8(_)
            | D::ImageRgba8(_)
            | D::ImageRgb16(_)
            | D::ImageRgba16(_) => img,
            D::ImageLumaA8(_) => D::ImageRgba8(img.to_rgba8()),
            D::ImageRgb32F(_) => D::ImageRgb16(img.to_rgb16()),
            _ => D::ImageRgba16(img.to_rgba16()),
        },
    }
}

/// Add a border of `p` pixels, filled with the background (or transparent
/// when the image keeps its alpha channel).
fn pad(img: DynamicImage, p: u32, bg: Rgb) -> DynamicImage {
    if p == 0 {
        return img;
    }
    let (w, h) = (img.width() + 2 * p, img.height() + 2 * p);
    if img.color().has_alpha() {
        let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 0]));
        image::imageops::overlay(&mut out, &img.to_rgba8(), i64::from(p), i64::from(p));
        DynamicImage::ImageRgba8(out)
    } else {
        let [r, g, b] = bg.0;
        let mut out = image::RgbImage::from_pixel(w, h, image::Rgb([r, g, b]));
        image::imageops::overlay(&mut out, &img.to_rgb8(), i64::from(p), i64::from(p));
        DynamicImage::ImageRgb8(out)
    }
}

/// Run the full editing pipeline and return pixels ready for `target`:
/// rotate → crop → tone → resize → annotations → redactions → background → padding.
pub fn process(img: DynamicImage, target: Format, opts: &Resolved) -> DynamicImage {
    let mut img = apply_edits(img, &opts.edits);
    if let Some(r) = &opts.resize {
        img = resize(img, r);
    }
    img = draw::render(img, &opts.annotations, &opts.redactions);
    if (opts.flatten || !target.writes_alpha()) && img.color().has_alpha() {
        img = flatten(&img, opts.background);
    }
    img = pad(img, opts.padding, opts.background);
    prepare(img, target, opts.background)
}

/// Lossless PNG optimisation (oxipng). Keeps colour-management chunks, drops
/// text and timestamps.
fn optimize_png(data: &[u8]) -> Result<Vec<u8>> {
    let mut o = oxipng::Options::from_preset(2);
    o.strip = oxipng::StripChunks::Safe;
    oxipng::optimize_from_memory(data, &o).map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("PNG optimisation failed: {e}."),
        )
    })
}

fn encode<W: Write + std::io::Seek>(
    img: &DynamicImage,
    target: Format,
    opts: &Resolved,
    icc: Option<&[u8]>,
    w: &mut W,
) -> Result<()> {
    let enc_err = |e| RadialError::from_image(e, "encoding the output");
    match target {
        Format::Png => {
            let mut buf = Vec::new();
            {
                let mut enc = PngEncoder::new_with_quality(
                    &mut buf,
                    CompressionType::Default,
                    PngFilter::Adaptive,
                );
                if let Some(p) = icc {
                    let _ = enc.set_icc_profile(p.to_vec());
                }
                img.write_with_encoder(enc).map_err(enc_err)?;
            }
            if opts.optimize {
                buf = optimize_png(&buf)?;
            }
            w.write_all(&buf)
                .map_err(|e| RadialError::from_io(&e, "writing the output"))
        }
        Format::Jpeg => {
            let mut enc = JpegEncoder::new_with_quality(w, opts.quality);
            if let Some(p) = icc {
                let _ = enc.set_icc_profile(p.to_vec());
            }
            img.write_with_encoder(enc).map_err(enc_err)
        }
        Format::Webp => {
            let (data, layout) = match img {
                DynamicImage::ImageRgba8(b) => (b.as_raw().as_slice(), webp::PixelLayout::Rgba),
                DynamicImage::ImageRgb8(b) => (b.as_raw().as_slice(), webp::PixelLayout::Rgb),
                _ => {
                    return Err(RadialError::new(
                        ErrorKind::Internal,
                        "WebP input was not prepared.",
                    ))
                }
            };
            let encoder = webp::Encoder::new(data, layout, img.width(), img.height());
            let mem = encoder
                .encode_simple(opts.lossless, f32::from(opts.quality))
                .map_err(|e| {
                    RadialError::new(
                        ErrorKind::EngineFailed,
                        format!("WebP encoding failed: {e:?}."),
                    )
                })?;
            w.write_all(&mem)
                .map_err(|e| RadialError::from_io(&e, "writing the output"))
        }
        Format::Tiff => encode_tiff(img, w),
        Format::Bmp | Format::Gif => img.write_to(w, target.codec()?).map_err(enc_err),
        Format::Pdf | Format::Txt | Format::Docx => Err(RadialError::new(
            ErrorKind::Internal,
            "Not an image target.",
        )),
    }
}

/// TIFF via the `tiff` crate directly: LZW with horizontal prediction (lossless
/// and widely supported), and an ExtraSamples tag marking the fourth channel as
/// unassociated alpha so other readers don't misinterpret RGBA.
fn encode_tiff<W: Write + std::io::Seek>(img: &DynamicImage, w: &mut W) -> Result<()> {
    use tiff::encoder::{colortype, Compression, Predictor, TiffEncoder};
    use tiff::tags::{ExtraSamples, Tag};
    let err = |e: tiff::TiffError| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("TIFF encoding failed: {e}."),
        )
    };
    let mut enc = TiffEncoder::new(w)
        .map_err(err)?
        .with_compression(Compression::Lzw)
        .with_predictor(Predictor::Horizontal);
    let (width, height) = (img.width(), img.height());
    macro_rules! write_image {
        ($color:ty, $data:expr, $alpha:expr) => {{
            let mut image = enc.new_image::<$color>(width, height).map_err(err)?;
            if $alpha {
                image
                    .encoder()
                    .write_tag(
                        Tag::ExtraSamples,
                        &[ExtraSamples::UnassociatedAlpha.to_u16()][..],
                    )
                    .map_err(err)?;
            }
            image.write_data($data).map_err(err)
        }};
    }
    match img {
        DynamicImage::ImageLuma8(b) => write_image!(colortype::Gray8, b.as_raw(), false),
        DynamicImage::ImageLuma16(b) => write_image!(colortype::Gray16, b.as_raw(), false),
        DynamicImage::ImageRgb8(b) => write_image!(colortype::RGB8, b.as_raw(), false),
        DynamicImage::ImageRgba8(b) => write_image!(colortype::RGBA8, b.as_raw(), true),
        DynamicImage::ImageRgb16(b) => write_image!(colortype::RGB16, b.as_raw(), false),
        DynamicImage::ImageRgba16(b) => write_image!(colortype::RGBA16, b.as_raw(), true),
        _ => Err(RadialError::new(
            ErrorKind::Internal,
            "TIFF input was not prepared.",
        )),
    }
}

/// Decode a written file and check it is the expected format and size.
pub fn verify(path: &Path, target: Format, width: u32, height: u32) -> Result<()> {
    let mut head = [0u8; 64];
    let n = {
        use std::io::Read;
        let mut f =
            File::open(path).map_err(|e| RadialError::from_io(&e, "verifying the output"))?;
        f.read(&mut head)
            .map_err(|e| RadialError::from_io(&e, "verifying the output"))?
    };
    if detect::sniff(&head[..n]) != Some(Recognized::Convertible(target)) {
        return Err(RadialError::new(
            ErrorKind::EngineFailed,
            "The output is not a valid file of the requested format.",
        ));
    }
    let loaded = load(path, target).map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("The output could not be read back: {}", e.message),
        )
    })?;
    if (loaded.image.width(), loaded.image.height()) != (width, height) {
        return Err(RadialError::new(
            ErrorKind::EngineFailed,
            "The output has unexpected dimensions.",
        ));
    }
    Ok(())
}

/// Fail with `Cancelled` if cancellation was requested.
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    check(cancel)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(RadialError::cancelled())
    } else {
        Ok(())
    }
}

/// Validate a request and return `(source format, resolved options, final path)`
/// without touching any pixels.
pub fn plan(registry: &Registry, req: &ConvertRequest) -> Result<(Format, Resolved, PathBuf)> {
    let source = paths::validate_source(&req.source)?;
    let info = detect::inspect(&source)?;
    if let Some(p) = info.problem {
        return Err(p);
    }
    let from = info.format.ok_or_else(|| {
        RadialError::new(ErrorKind::UnsupportedFormat, "Unsupported source file.")
    })?;
    let cap = registry.find(from, req.target).ok_or_else(|| {
        RadialError::new(
            ErrorKind::UnsupportedConversion,
            format!(
                "{} → {} is not a supported conversion.",
                from.label(),
                req.target.label()
            ),
        )
    })?;
    if !cap.available {
        return Err(RadialError::new(
            ErrorKind::EngineMissing,
            format!("{} is not installed.", cap.engine_label),
        ));
    }
    let resolved = req.options.validate(req.target)?;
    let out = output::plan(&source, &req.dest_dir, &req.stem, req.target.extension())?;
    Ok((from, resolved, out))
}

/// Result of [`image_job`].
pub struct ImageResult {
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

/// The shared image pipeline: decode → process → encode into a staged file →
/// verify → persist. When `must_shrink_below` is set, the result is discarded
/// (nothing written) unless it is smaller than that many bytes.
#[allow(clippy::too_many_arguments)]
pub fn image_job(
    source: &Path,
    from: Format,
    target: Format,
    resolved: &Resolved,
    out: &Path,
    must_shrink_below: Option<u64>,
    cancel: &AtomicBool,
    mut on_stage: impl FnMut(Stage),
) -> Result<ImageResult> {
    check(cancel)?;
    on_stage(Stage::Decoding);
    let Loaded { image, icc } = load(source, from)?;
    encode_job(
        image,
        icc.as_deref(),
        target,
        resolved,
        out,
        must_shrink_below,
        cancel,
        on_stage,
    )
}

/// The second half of [`image_job`] for an already decoded image:
/// process → encode → verify → persist.
#[allow(clippy::too_many_arguments)]
pub fn encode_job(
    image: DynamicImage,
    icc: Option<&[u8]>,
    target: Format,
    resolved: &Resolved,
    out: &Path,
    must_shrink_below: Option<u64>,
    cancel: &AtomicBool,
    mut on_stage: impl FnMut(Stage),
) -> Result<ImageResult> {
    check(cancel)?;
    on_stage(Stage::Processing);
    let image = process(image, target, resolved);
    let (width, height) = (image.width(), image.height());

    check(cancel)?;
    on_stage(Stage::Encoding);
    let tmp = output::stage(out)?;
    {
        let mut w = BufWriter::new(tmp.as_file());
        encode(&image, target, resolved, icc, &mut w)?;
        w.flush()
            .map_err(|e| RadialError::from_io(&e, "writing the output"))?;
    }
    output::sync(&tmp)?;
    drop(image);

    check(cancel)?;
    on_stage(Stage::Verifying);
    verify(tmp.path(), target, width, height)?;
    if let Some(limit) = must_shrink_below {
        let size = output::staged_size(&tmp);
        if size >= limit {
            return Err(RadialError::new(
                ErrorKind::NoReduction,
                format!(
                    "These settings would not make the file smaller ({} → {} bytes), so nothing was saved. Try a lower quality or a smaller size.",
                    limit, size
                ),
            ));
        }
    }

    check(cancel)?;
    let size_bytes = output::persist(tmp, out)?;
    Ok(ImageResult {
        width,
        height,
        size_bytes,
    })
}

/// Perform a conversion. `on_stage` is called as each stage begins.
pub fn convert(
    registry: &Registry,
    req: &ConvertRequest,
    cancel: &AtomicBool,
    on_stage: impl FnMut(Stage),
) -> Result<ConvertOutcome> {
    let (from, resolved, out) = plan(registry, req)?;
    let r = image_job(
        &req.source,
        from,
        req.target,
        &resolved,
        &out,
        None,
        cancel,
        on_stage,
    )?;
    Ok(ConvertOutcome {
        output_path: out,
        source_format: from,
        format: req.target,
        width: r.width,
        height: r.height,
        size_bytes: r.size_bytes,
    })
}

/// A downscaled, orientation-corrected copy of an image (or of one PDF page)
/// for on-screen preview.
pub fn preview_base(path: &Path, page: u32, max_dim: u32) -> Result<DynamicImage> {
    let info = detect::inspect(path)?;
    if let Some(p) = info.problem {
        return Err(p);
    }
    let format = info.format.ok_or_else(|| {
        RadialError::new(ErrorKind::UnsupportedFormat, "Cannot preview this file.")
    })?;
    let max_dim = max_dim.clamp(16, limits::MAX_PREVIEW_DIMENSION);
    if format == Format::Pdf {
        return crate::pdf::render_page(path, page, max_dim);
    }
    let img = load(path, format)?.image;
    Ok(if img.width() > max_dim || img.height() > max_dim {
        img.resize(max_dim, max_dim, FilterType::Triangle)
    } else {
        img
    })
}

/// Apply edits to a preview base and encode it for display: JPEG when opaque
/// (fast, small), PNG when it has transparency.
pub fn render_preview(base: &DynamicImage, edits: &Edits) -> Result<Vec<u8>> {
    let img = if edits.is_identity() {
        base.clone()
    } else {
        apply_edits(base.clone(), edits)
    };
    let mut out = std::io::Cursor::new(Vec::new());
    let err = |e| RadialError::from_image(e, "rendering the preview");
    if img.color().has_alpha() {
        let enc =
            PngEncoder::new_with_quality(&mut out, CompressionType::Fast, PngFilter::Adaptive);
        DynamicImage::ImageRgba8(img.to_rgba8())
            .write_with_encoder(enc)
            .map_err(err)?;
    } else {
        let enc = JpegEncoder::new_with_quality(&mut out, 88);
        DynamicImage::ImageRgb8(img.to_rgb8())
            .write_with_encoder(enc)
            .map_err(err)?;
    }
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::CropRect;

    fn grad(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, 255])
        }))
    }

    #[test]
    fn rotate_then_crop_uses_rotated_space() {
        let e = Edits {
            rotate: 90,
            crop: Some(CropRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            }),
            ..Default::default()
        };
        let out = apply_edits(grad(200, 100), &e);
        assert_eq!((out.width(), out.height()), (50, 200));
    }

    #[test]
    fn tone_adjustment_preserves_alpha_and_depth() {
        let mut img = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
            2,
            2,
            image::Rgba([30000u16, 30000, 30000, 1234]),
        ));
        adjust_tone(&mut img, 40, 0);
        let DynamicImage::ImageRgba16(b) = &img else {
            panic!("depth changed")
        };
        let p = b.get_pixel(0, 0);
        assert!(p[0] > 30000);
        assert_eq!(p[3], 1234);
    }

    #[test]
    fn flatten_blends_with_background() {
        let img = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([0, 0, 0, 0]),
        ));
        let out = flatten(&img, Rgb([10, 20, 30]));
        assert_eq!(out.to_rgb8().get_pixel(0, 0).0, [10, 20, 30]);
        let half = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([255, 255, 255, 128]),
        ));
        let out = flatten(&half, Rgb([0, 0, 0]));
        assert_eq!(out.to_rgb8().get_pixel(0, 0).0, [128, 128, 128]);
    }
}
