//! PDF compression: recompress JPEG photos, compress uncompressed streams,
//! drop page thumbnails and unreferenced objects, and pack objects into
//! compressed object streams. Text and vector graphics are not touched.

use super::{char_counts, engine, load_lopdf, open, plural, save_lopdf};
use crate::engine::check_cancel;
use crate::error::{ErrorKind, RadialError, Result};
use crate::limits;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder};
use lopdf::{Dictionary, Object};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Only photos at least this large are worth re-encoding.
const MIN_IMAGE_BYTES: usize = 4096;

fn name_is(d: &Dictionary, key: &[u8], value: &[u8]) -> bool {
    match d.get(key) {
        Ok(Object::Name(n)) => n == value,
        Ok(Object::Array(a)) if a.len() == 1 => matches!(&a[0], Object::Name(n) if n == value),
        _ => false,
    }
}

/// Grey or RGB colour space that a re-encoded baseline JPEG can keep.
fn simple_colorspace(doc: &lopdf::Document, d: &Dictionary) -> Option<u8> {
    let cs = d.get(b"ColorSpace").ok()?;
    let (_, cs) = doc.dereference(cs).ok()?;
    match cs {
        Object::Name(n) if n == b"DeviceRGB" => Some(3),
        Object::Name(n) if n == b"DeviceGray" => Some(1),
        Object::Array(a)
            if a.len() == 2 && matches!(&a[0], Object::Name(n) if n == b"ICCBased") =>
        {
            let (_, icc) = doc.dereference(&a[1]).ok()?;
            match icc.as_stream().ok()?.dict.get(b"N").ok()?.as_i64().ok()? {
                1 => Some(1),
                3 => Some(3),
                _ => None,
            }
        }
        _ => None,
    }
}

fn decode_jpeg(data: &[u8]) -> Option<DynamicImage> {
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Jpeg);
    reader.limits(limits::decode_limits());
    let decoder = reader.into_decoder().ok()?;
    // CMYK and other layouts can't be stored back with the same colour space.
    if !matches!(
        decoder.color_type(),
        image::ColorType::L8 | image::ColorType::Rgb8
    ) {
        return None;
    }
    DynamicImage::from_decoder(decoder).ok()
}

pub fn compress(
    source: &Path,
    quality: u8,
    max_dimension: Option<u32>,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<String>)> {
    let data = std::fs::read(source).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    let pdfium = engine()?;
    let (pages, before) = {
        let d = open(pdfium, source)?;
        (d.pages().len() as u32, char_counts(&d))
    };
    let mut doc = load_lopdf(&data)?;
    if doc.is_encrypted() {
        return Err(RadialError::new(
            ErrorKind::UnsupportedFormat,
            "Encrypted PDFs can't be compressed.",
        ));
    }

    let ids: Vec<_> = doc.objects.keys().copied().collect();
    let (mut recompressed, mut resized) = (0usize, 0usize);
    for id in ids {
        check_cancel(cancel)?;
        let candidate = {
            let Some(Object::Stream(s)) = doc.objects.get(&id) else {
                continue;
            };
            let d = &s.dict;
            if !name_is(d, b"Subtype", b"Image")
                || !name_is(d, b"Filter", b"DCTDecode")
                || d.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8)
                || d.has(b"Decode")
                || s.content.len() < MIN_IMAGE_BYTES
            {
                continue;
            }
            let Some(channels) = simple_colorspace(&doc, d) else {
                continue;
            };
            let masked = d.has(b"SMask") || d.has(b"Mask");
            Some((s.content.clone(), channels, masked))
        };
        let Some((content, channels, masked)) = candidate else {
            continue;
        };
        let Some(mut img) = decode_jpeg(&content) else {
            continue;
        };
        if usize::from(img.color().channel_count()) != usize::from(channels) {
            continue;
        }
        let mut did_resize = false;
        // A mask must match the image's size, so masked images keep theirs.
        if let Some(m) = max_dimension.filter(|_| !masked) {
            if img.width() > m || img.height() > m {
                img = img.resize(m, m, FilterType::Lanczos3);
                did_resize = true;
            }
        }
        let mut out = Vec::new();
        let enc = JpegEncoder::new_with_quality(&mut out, quality);
        let ok = if channels == 1 {
            DynamicImage::ImageLuma8(img.to_luma8()).write_with_encoder(enc)
        } else {
            DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(enc)
        };
        if ok.is_err() || out.len() >= content.len() * 95 / 100 {
            continue;
        }
        if let Some(Object::Stream(s)) = doc.objects.get_mut(&id) {
            s.dict.set("Width", i64::from(img.width()));
            s.dict.set("Height", i64::from(img.height()));
            s.dict.remove(b"DecodeParms");
            s.dict.remove(b"Length");
            s.set_content(out);
            recompressed += 1;
            resized += usize::from(did_resize);
        }
    }

    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    for id in page_ids {
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.remove(b"Thumb");
        }
    }
    doc.prune_objects();
    check_cancel(cancel)?;
    let bytes = save_lopdf(&mut doc, true)?;

    if bytes.len() as u64 >= data.len() as u64 {
        return Err(RadialError::new(
            ErrorKind::NoReduction,
            format!(
                "This PDF is already compact ({} → {} bytes with these settings), so nothing was saved. Try a lower quality or a smaller image size.",
                data.len(),
                bytes.len()
            ),
        ));
    }
    let after = super::verify(&bytes, pages)?;
    if after != before {
        return Err(RadialError::new(
            ErrorKind::EngineFailed,
            "The compressed PDF's text didn't match the original, so nothing was saved.",
        ));
    }

    let mut notes = Vec::new();
    if recompressed > 0 {
        notes.push(format!(
            "{} re-encoded as JPEG at quality {quality}{}.",
            plural(recompressed, "photo", "photos"),
            if resized > 0 {
                format!(" ({resized} also reduced in size)")
            } else {
                String::new()
            }
        ));
    } else {
        notes.push("No photos needed re-encoding.".into());
    }
    notes.push("File structure compressed; text and vector graphics are unchanged.".into());
    Ok((bytes, notes))
}
