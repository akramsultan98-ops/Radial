//! Genuine PDF redaction.
//!
//! PDF text can't be removed reliably by deleting individual glyphs: one
//! text object often holds a whole line, fonts may be subset or re-encoded,
//! and copies of the text can hide in annotations, form values or tagged
//! structure. So RADIAL redacts a page by *replacing it with an image of
//! itself* in which the marked areas are already black:
//!
//! 1. Areas are collected from the user's boxes and from text search.
//! 2. Each affected page is rendered (with annotations and form fields),
//!    the areas are painted black in the pixels, every original object and
//!    annotation is deleted from the page, and the image is placed instead.
//! 3. The file is rebuilt from scratch with lopdf: unreferenced objects (the
//!    old content, fonts and images), orphaned form fields, page thumbnails,
//!    tagged-structure text and document metadata are removed.
//! 4. The result is verified: redacted pages contain no text, the search
//!    terms occur nowhere in the text of any page or in any string in the
//!    file, and every page still renders.
//!
//! Pages without redactions are left as they were.

use super::{
    engine, finalize_doc, open, page, page_rect, pdf_err, pixel_size, plural, render,
    render_config, save, PdfOptions, REDACT_DPI,
};
use crate::draw;
use crate::engine::check_cancel;
use crate::error::{ErrorKind, RadialError, Result};
use image::{DynamicImage, Rgb};
use lopdf::{dictionary, Dictionary, Object, ObjectId};
use pdfium_render::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn invalid(m: impl Into<String>) -> RadialError {
    RadialError::new(ErrorKind::InvalidOptions, m)
}

/// Areas in user space, by page.
type Areas = BTreeMap<u32, Vec<PdfRect>>;

/// Redact `source`. Returns the new file and notes for the user.
pub fn redact(
    source: &Path,
    opts: &PdfOptions,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<String>)> {
    let terms = opts.clean_terms()?;
    let rects: Vec<_> = opts.regions.iter().map(|r| r.rect).collect();
    draw::validate_regions(&rects)?;
    if opts.regions.is_empty() && terms.is_empty() {
        return Err(invalid(
            "Mark an area or enter text to find before redacting.",
        ));
    }

    let pdfium = engine()?;
    let doc = open(pdfium, source)?;
    let pages = doc.pages().len() as u32;

    let mut areas: Areas = BTreeMap::new();
    for r in &opts.regions {
        let p = page(&doc, r.page)?;
        areas
            .entry(r.page)
            .or_default()
            .push(page_rect(&p, &r.rect)?);
    }
    let mut hits = 0usize;
    if !terms.is_empty() {
        let search = PdfSearchOptions::new();
        for (i, p) in doc.pages().iter().enumerate() {
            check_cancel(cancel)?;
            let text = p.text().map_err(|e| pdf_err("reading text", e))?;
            for t in &terms {
                let s = text
                    .search(t, &search)
                    .map_err(|e| pdf_err("searching text", e))?;
                for segments in s.iter(PdfSearchDirection::SearchForward) {
                    hits += 1;
                    for seg in segments.iter() {
                        areas
                            .entry(i as u32)
                            .or_default()
                            .push(grow(seg.bounds(), 1.0));
                    }
                }
            }
        }
        if hits == 0 && opts.regions.is_empty() {
            // The words may still be in the file outside page text.
            if let Some(found) = find_in_file(source, &terms)? {
                return Err(RadialError::new(
                    ErrorKind::UnsupportedConversion,
                    format!(
                        "“{found}” isn't in the text of any page, but it appears elsewhere in the file (for example a bookmark, form value or attachment name), which RADIAL can't redact. Nothing was saved."
                    ),
                ));
            }
            return Err(invalid(
                "None of the search terms were found in the PDF's text. Nothing was changed. (Scanned pages have no text to search; mark those areas by hand.)",
            ));
        }
    }

    let before = super::char_counts(&doc);
    for (&index, rects) in &areas {
        check_cancel(cancel)?;
        let mut p = page(&doc, index)?;
        flatten_page(&doc, &mut p, rects)?;
    }
    let bytes = save(&doc)?;
    drop(doc);

    check_cancel(cancel)?;
    let redacted: BTreeSet<u32> = areas.keys().copied().collect();
    let (bytes, removed_fields, removed_tags) = rebuild(&bytes, &redacted)?;

    check_cancel(cancel)?;
    verify(&bytes, pages, &redacted, &terms, &before)?;

    let list: Vec<u32> = redacted.iter().copied().collect();
    let n_areas: usize = areas.values().map(Vec::len).sum();
    let mut notes = vec![format!(
        "{} removed on page{} {}. Those pages are now images ({REDACT_DPI} DPI), so their text can't be selected or searched; other pages are unchanged.",
        plural(n_areas, "area", "areas"),
        if list.len() == 1 { "" } else { "s" },
        super::page_list(&list),
    )];
    if !terms.is_empty() {
        notes.push(format!(
            "Search found {} of the text you entered.",
            plural(hits, "match", "matches")
        ));
    }
    notes.push("Document metadata (title, author, XMP) was removed.".into());
    if removed_fields > 0 {
        notes.push(format!(
            "{} on the redacted pages removed.",
            plural(removed_fields, "form field", "form fields")
        ));
    }
    if removed_tags {
        notes.push(
            "Accessibility tags were removed because they can hold copies of redacted text.".into(),
        );
    }
    Ok((bytes, notes))
}

fn grow(r: PdfRect, by: f32) -> PdfRect {
    PdfRect::new_from_values(
        r.bottom().value - by,
        r.left().value - by,
        r.top().value + by,
        r.right().value + by,
    )
}

/// Replace a page's content with a rendered image in which `rects` are black.
fn flatten_page<'a>(doc: &PdfDocument<'a>, p: &mut PdfPage<'a>, rects: &[PdfRect]) -> Result<()> {
    let (w, h) = pixel_size(p, REDACT_DPI);
    let mut img = render(p, w, h)?.to_rgb8();
    let cfg = render_config(w, h);
    for r in rects {
        let corners = [
            (r.left(), r.bottom()),
            (r.left(), r.top()),
            (r.right(), r.bottom()),
            (r.right(), r.top()),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (x, y) in corners {
            let (px, py) = p
                .points_to_pixels(x, y, &cfg)
                .map_err(|e| pdf_err("mapping an area", e))?;
            x0 = x0.min(px);
            y0 = y0.min(py);
            x1 = x1.max(px);
            y1 = y1.max(py);
        }
        // Round outward by a pixel so anti-aliased glyph edges are covered too.
        let x0 = (x0 - 1).clamp(0, w) as u32;
        let y0 = (y0 - 1).clamp(0, h) as u32;
        let x1 = (x1 + 1).clamp(0, w) as u32;
        let y1 = (y1 + 1).clamp(0, h) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
    }

    // The bitmap shows the page as displayed; the page's content stream is
    // drawn before /Rotate is applied, so turn the image back.
    let img = DynamicImage::ImageRgb8(img);
    let img = match p.rotation().map_err(|e| pdf_err("reading rotation", e))? {
        PdfPageRenderRotation::Degrees90 => img.rotate270(),
        PdfPageRenderRotation::Degrees180 => img.rotate180(),
        PdfPageRenderRotation::Degrees270 => img.rotate90(),
        PdfPageRenderRotation::None => img,
    };

    let bounds = p
        .boundaries()
        .crop()
        .or_else(|_| p.boundaries().media())
        .map_err(|e| pdf_err("reading the page size", e))?
        .bounds;

    let objects = p.objects_mut();
    while objects.len() > 0 {
        objects
            .remove_object_at_index(0)
            .map_err(|e| pdf_err("removing page content", e))?;
    }
    let annotations = p.annotations_mut();
    while !annotations.is_empty() {
        let a = annotations
            .get(0)
            .map_err(|e| pdf_err("removing annotations", e))?;
        annotations
            .delete_annotation(a)
            .map_err(|e| pdf_err("removing annotations", e))?;
    }

    let mut obj = PdfPageImageObject::new_with_size(doc, &img, bounds.width(), bounds.height())
        .map_err(|e| pdf_err("placing the redacted page", e))?;
    obj.translate(bounds.left(), bounds.bottom())
        .map_err(|e| pdf_err("placing the redacted page", e))?;
    p.objects_mut()
        .add_image_object(obj)
        .map_err(|e| pdf_err("placing the redacted page", e))?;
    p.regenerate_content()
        .map_err(|e| pdf_err("writing the redacted page", e))?;
    Ok(())
}

/// Rebuild the file, dropping everything that could still hold redacted
/// content. Returns (bytes, form fields removed, tags removed).
fn rebuild(data: &[u8], redacted: &BTreeSet<u32>) -> Result<(Vec<u8>, usize, bool)> {
    let mut doc = super::load_lopdf(data)?;
    let pages = doc.get_pages();
    for (num, id) in &pages {
        if let Ok(d) = doc.get_dictionary_mut(*id) {
            // Thumbnails and private application data are copies of the page.
            d.remove(b"Thumb");
            d.remove(b"PieceInfo");
            d.remove(b"Metadata");
            if redacted.contains(&(num - 1)) {
                d.remove(b"Annots");
                d.remove(b"StructParents");
            }
        }
        if redacted.contains(&(num - 1)) {
            compact_page_images(&mut doc, *id);
        }
    }
    let removed_fields = prune_form_fields(&mut doc);
    let mut removed_tags = false;
    if let Ok(cat) = doc.catalog_mut() {
        removed_tags = cat.remove(b"StructTreeRoot").is_some();
        cat.remove(b"MarkInfo");
        cat.remove(b"PieceInfo");
    }
    let bytes = finalize_doc(doc, true, true)?;
    Ok((bytes, removed_fields, removed_tags))
}

/// The page images PDFium writes are RGB plus an all-opaque alpha mask,
/// deflated without a predictor. Drop the mask, store grey pages as grey, and
/// use PNG (Paeth) prediction, which typically shrinks them several times.
fn compact_page_images(doc: &mut lopdf::Document, page_id: ObjectId) {
    let ids: Vec<ObjectId> = doc
        .get_page_resources(page_id)
        .ok()
        .and_then(|(res, _)| res.and_then(|r| r.get(b"XObject").ok()).cloned())
        .and_then(|x| match x {
            Object::Dictionary(d) => Some(d),
            Object::Reference(id) => doc.get_dictionary(id).ok().cloned(),
            _ => None,
        })
        .map(|d| {
            d.iter()
                .filter_map(|(_, v)| v.as_reference().ok())
                .collect()
        })
        .unwrap_or_default();
    for id in ids {
        let Some(Object::Stream(s)) = doc.objects.get(&id) else {
            continue;
        };
        let d = &s.dict;
        let (Ok(w), Ok(h)) = (
            d.get(b"Width").and_then(Object::as_i64),
            d.get(b"Height").and_then(Object::as_i64),
        ) else {
            continue;
        };
        if !matches!(d.get(b"ColorSpace"), Ok(Object::Name(n)) if n == b"DeviceRGB")
            || d.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8)
            || d.has(b"DecodeParms")
        {
            continue;
        }
        let Ok(rgb) = s.decompressed_content() else {
            continue;
        };
        let (w, h) = (w as usize, h as usize);
        if rgb.len() != w * h * 3 {
            continue;
        }
        let opaque_mask = match d.get(b"SMask") {
            Ok(Object::Reference(m)) => match doc.objects.get(m) {
                Some(Object::Stream(ms)) => ms
                    .decompressed_content()
                    .is_ok_and(|a| a.len() == w * h && a.iter().all(|&v| v == 255)),
                _ => false,
            },
            Err(_) => true,
            _ => false,
        };
        if !opaque_mask {
            continue;
        }
        let px = rgb.as_chunks::<3>().0;
        let grey = px.iter().all(|p| p[0] == p[1] && p[1] == p[2]);
        let (pixels, channels): (Vec<u8>, usize) = if grey {
            (px.iter().map(|p| p[0]).collect(), 1)
        } else {
            (rgb, 3)
        };
        let Some(data) = paeth_deflate(&pixels, w, h, channels) else {
            continue;
        };
        if let Some(Object::Stream(s)) = doc.objects.get_mut(&id) {
            s.dict.remove(b"SMask");
            s.dict
                .set("ColorSpace", if grey { "DeviceGray" } else { "DeviceRGB" });
            s.dict.set("Filter", "FlateDecode");
            s.dict.set(
                "DecodeParms",
                dictionary! {
                    "Predictor" => 15,
                    "Colors" => channels as i64,
                    "BitsPerComponent" => 8,
                    "Columns" => w as i64,
                },
            );
            s.set_content(data);
            s.allows_compression = false;
        }
    }
}

/// Deflate pixels with the PNG Paeth filter on every row.
fn paeth_deflate(pixels: &[u8], w: usize, h: usize, bpp: usize) -> Option<Vec<u8>> {
    use std::io::Write;
    let stride = w * bpp;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    let mut row = vec![0u8; stride + 1];
    for y in 0..h {
        let cur = &pixels[y * stride..(y + 1) * stride];
        let prev = (y > 0).then(|| &pixels[(y - 1) * stride..y * stride]);
        row[0] = 4;
        for i in 0..stride {
            let a = if i >= bpp { i16::from(cur[i - bpp]) } else { 0 };
            let b = prev.map_or(0, |p| i16::from(p[i]));
            let c = match prev {
                Some(p) if i >= bpp => i16::from(p[i - bpp]),
                _ => 0,
            };
            let est = a + b - c;
            let (pa, pb, pc) = ((est - a).abs(), (est - b).abs(), (est - c).abs());
            let pred = if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            };
            row[i + 1] = cur[i].wrapping_sub(pred as u8);
        }
        enc.write_all(&row).ok()?;
    }
    enc.finish().ok()
}

/// Remove form fields with no widget left on any page (their values would
/// otherwise survive in the file).
fn prune_form_fields(doc: &mut lopdf::Document) -> usize {
    let mut live: BTreeSet<ObjectId> = BTreeSet::new();
    for id in doc.get_pages().values() {
        if let Ok(d) = doc.get_dictionary(*id) {
            if let Ok(arr) = d
                .get(b"Annots")
                .and_then(|a| doc.dereference(a))
                .and_then(|(_, a)| a.as_array())
            {
                live.extend(arr.iter().filter_map(|o| o.as_reference().ok()));
            }
        }
    }
    let acro_ref = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok().cloned());
    let Some(acro_obj) = acro_ref else { return 0 };
    let fields: Vec<Object> = match &acro_obj {
        Object::Reference(id) => doc
            .get_dictionary(*id)
            .ok()
            .and_then(|d| d.get(b"Fields").ok())
            .and_then(|f| doc.dereference(f).ok())
            .and_then(|(_, f)| f.as_array().ok().cloned())
            .unwrap_or_default(),
        Object::Dictionary(d) => d
            .get(b"Fields")
            .ok()
            .and_then(|f| doc.dereference(f).ok())
            .and_then(|(_, f)| f.as_array().ok().cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut removed = 0;
    let kept: Vec<Object> = fields
        .into_iter()
        .filter(|f| {
            let keep = f
                .as_reference()
                .map(|id| field_is_live(doc, id, &live, 0))
                .unwrap_or(true);
            if !keep {
                removed += 1;
            }
            keep
        })
        .collect();
    if removed > 0 {
        let set = |d: &mut Dictionary| d.set("Fields", Object::Array(kept.clone()));
        match acro_obj {
            Object::Reference(id) => {
                if let Ok(d) = doc.get_dictionary_mut(id) {
                    set(d);
                }
            }
            Object::Dictionary(_) => {
                if let Ok(Object::Dictionary(d)) = doc
                    .catalog_mut()
                    .map(|c| c.get_mut(b"AcroForm"))
                    .and_then(|r| r)
                {
                    set(d);
                }
            }
            _ => {}
        }
    }
    removed
}

fn field_is_live(
    doc: &lopdf::Document,
    id: ObjectId,
    live: &BTreeSet<ObjectId>,
    depth: u8,
) -> bool {
    if live.contains(&id) || depth > 32 {
        return true;
    }
    let Ok(d) = doc.get_dictionary(id) else {
        return false;
    };
    match d
        .get(b"Kids")
        .and_then(|k| doc.dereference(k))
        .and_then(|(_, k)| k.as_array())
    {
        Ok(kids) => kids
            .iter()
            .filter_map(|k| k.as_reference().ok())
            .any(|k| field_is_live(doc, k, live, depth + 1)),
        Err(_) => false,
    }
}

fn verify(
    data: &[u8],
    pages: u32,
    redacted: &BTreeSet<u32>,
    terms: &[String],
    before: &[usize],
) -> Result<()> {
    let fail = |m: String| RadialError::new(ErrorKind::EngineFailed, m);
    let chars = super::verify(data, pages)?;
    for (i, (&now, &was)) in chars.iter().zip(before).enumerate() {
        let i = i as u32;
        if redacted.contains(&i) {
            if now != 0 {
                return Err(fail(format!(
                    "Redaction failed verification: text remains on page {}. Nothing was saved.",
                    i + 1
                )));
            }
        } else if now != was {
            return Err(fail(format!(
                "Redaction failed verification: page {} changed unexpectedly. Nothing was saved.",
                i + 1
            )));
        }
    }
    if terms.is_empty() {
        return Ok(());
    }
    let pdfium = engine()?;
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| pdf_err("verifying the result", e))?;
    let search = PdfSearchOptions::new();
    for (i, p) in doc.pages().iter().enumerate() {
        let text = p.text().map_err(|e| pdf_err("verifying the result", e))?;
        for t in terms {
            let s = text
                .search(t, &search)
                .map_err(|e| pdf_err("verifying the result", e))?;
            if s.find_next().is_some() {
                return Err(fail(format!(
                    "Redaction failed verification: the text still appears on page {}. Nothing was saved.",
                    i + 1
                )));
            }
        }
    }
    drop(doc);
    // Bookmarks, annotations elsewhere, form values, attachment names…
    let doc = super::load_lopdf(data)?;
    let lowered: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    for obj in doc.objects.values() {
        if let Some(found) = find_string(obj, &lowered, 0) {
            return Err(RadialError::new(
                ErrorKind::EngineFailed,
                format!(
                    "“{found}” also appears outside the page content (for example in a bookmark, link, form value or attachment name), which RADIAL can't redact. Nothing was saved."
                ),
            ));
        }
    }
    Ok(())
}

/// Search every string object of a PDF for any of `terms` (case-insensitive).
fn find_in_file(path: &Path, terms: &[String]) -> Result<Option<String>> {
    let data = std::fs::read(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    let Ok(doc) = super::load_lopdf(&data) else {
        return Ok(None);
    };
    let lowered: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    Ok(doc
        .objects
        .values()
        .find_map(|o| find_string(o, &lowered, 0)))
}

fn find_string(obj: &Object, terms: &[String], depth: u8) -> Option<String> {
    if depth > 64 {
        return None;
    }
    match obj {
        Object::String(bytes, _) => {
            let s = super::meta::decode_text(bytes).to_lowercase();
            terms.iter().find(|t| s.contains(t.as_str())).cloned()
        }
        Object::Array(a) => a.iter().find_map(|o| find_string(o, terms, depth + 1)),
        Object::Dictionary(d) => d.iter().find_map(|(_, o)| find_string(o, terms, depth + 1)),
        Object::Stream(s) => s
            .dict
            .iter()
            .find_map(|(_, o)| find_string(o, terms, depth + 1)),
        _ => None,
    }
}
