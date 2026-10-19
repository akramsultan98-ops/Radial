//! PDF ↔ other formats: page → image, text extraction, image → PDF.

use super::{engine, open, page, pixel_size, render};
use crate::engine::check_cancel;
use crate::error::{ErrorKind, RadialError, Result};
use crate::format::Format;
use image::DynamicImage;
use lopdf::{dictionary, Object, Stream};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Render one page at `dpi`.
pub fn to_image(source: &Path, index: u32, dpi: u32) -> Result<DynamicImage> {
    let pdfium = engine()?;
    let doc = open(pdfium, source)?;
    let p = page(&doc, index)?;
    let (w, h) = pixel_size(&p, dpi);
    render(&p, w, h)
}

/// Extract the text of every page, pages separated by a blank line.
/// Fails when the PDF has no text layer (e.g. scanned pages).
pub fn to_text(source: &Path, cancel: &AtomicBool) -> Result<(String, usize)> {
    let pdfium = engine()?;
    let doc = open(pdfium, source)?;
    let mut out = String::new();
    let mut with_text = 0;
    for p in doc.pages().iter() {
        check_cancel(cancel)?;
        let t = super::page_text(&p)?;
        let t = t.trim_end();
        if !t.trim().is_empty() {
            with_text += 1;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(t);
    }
    if with_text == 0 {
        return Err(RadialError::new(
            ErrorKind::UnsupportedConversion,
            "This PDF has no selectable text (it may be scanned). RADIAL doesn't do text recognition (OCR).",
        ));
    }
    out.push('\n');
    Ok((out, with_text))
}

const A4: (f32, f32) = (595.276, 841.89);
const MARGIN: f32 = 36.0;

/// Bytes of a JPEG that can be embedded as-is: 8-bit grey or RGB with no
/// EXIF rotation to apply.
fn jpeg_passthrough(path: &Path) -> Option<(Vec<u8>, u32, u32, &'static str)> {
    use image::ImageDecoder;
    let data = std::fs::read(path).ok()?;
    let mut d = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&data)).ok()?;
    let orientation = d.orientation().ok()?;
    if orientation != image::metadata::Orientation::NoTransforms {
        return None;
    }
    let cs = match d.color_type() {
        image::ColorType::L8 => "DeviceGray",
        image::ColorType::Rgb8 => "DeviceRGB",
        _ => return None,
    };
    let (w, h) = d.dimensions();
    Some((data, w, h, cs))
}

/// Place an image on one A4 page (portrait or landscape to match), centred
/// within a margin and never enlarged beyond 96 DPI.
pub fn image_to_pdf(source: &Path, from: Format) -> Result<(Vec<u8>, Vec<String>)> {
    let mut doc = lopdf::Document::with_version("1.7");
    let mut notes = Vec::new();

    let (image_id, w, h) = match (from == Format::Jpeg)
        .then(|| jpeg_passthrough(source))
        .flatten()
    {
        Some((data, w, h, cs)) => {
            let s = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image",
                    "Width" => i64::from(w), "Height" => i64::from(h),
                    "ColorSpace" => cs, "BitsPerComponent" => 8,
                    "Filter" => "DCTDecode",
                },
                data,
            )
            .with_compression(false);
            notes.push("The JPEG was embedded unchanged (no quality loss).".into());
            (doc.add_object(s), w, h)
        }
        None => {
            let img = crate::engine::load(source, from)?.image;
            let (w, h) = (img.width(), img.height());
            let alpha = img.color().has_alpha();
            let gray = matches!(
                img.color(),
                image::ColorType::L8
                    | image::ColorType::L16
                    | image::ColorType::La8
                    | image::ColorType::La16
            );
            let (pixels, cs) = if gray {
                (img.to_luma8().into_raw(), "DeviceGray")
            } else {
                (img.to_rgb8().into_raw(), "DeviceRGB")
            };
            let mut dict = dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => i64::from(w), "Height" => i64::from(h),
                "ColorSpace" => cs, "BitsPerComponent" => 8,
            };
            if alpha {
                let a: Vec<u8> = img.to_rgba8().pixels().map(|p| p.0[3]).collect();
                let mut m = Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Image",
                        "Width" => i64::from(w), "Height" => i64::from(h),
                        "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                    },
                    a,
                );
                let _ = m.compress();
                dict.set("SMask", doc.add_object(m));
                notes.push("Transparency is kept.".into());
            }
            let mut s = Stream::new(dict, pixels);
            s.compress().map_err(|e| {
                RadialError::new(
                    ErrorKind::EngineFailed,
                    format!("Compressing the image failed ({e})."),
                )
            })?;
            (doc.add_object(s), w, h)
        }
    };

    let (pw, ph) = if w > h { (A4.1, A4.0) } else { A4 };
    let natural = (w as f32 * 0.75, h as f32 * 0.75);
    let scale = ((pw - 2.0 * MARGIN) / natural.0)
        .min((ph - 2.0 * MARGIN) / natural.1)
        .min(1.0);
    let (iw, ih) = (natural.0 * scale, natural.1 * scale);
    let (x, y) = ((pw - iw) / 2.0, (ph - ih) / 2.0);
    let content = format!("q {iw:.3} 0 0 {ih:.3} {x:.3} {y:.3} cm /Im0 Do Q").into_bytes();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content));

    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(pw), Object::Real(ph)],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let bytes = super::save_lopdf(&mut doc, true)?;
    notes.push(format!(
        "Placed on an A4 {} page.",
        if w > h { "landscape" } else { "portrait" }
    ));
    Ok((bytes, notes))
}
