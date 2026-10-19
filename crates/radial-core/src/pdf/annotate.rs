//! Annotate (flattened vector drawing) and Crop for PDFs.

use super::{
    char_counts, engine, finalize_doc, load_lopdf, open, page, page_point, page_rect, pdf_err,
    plural, save, short_side, PdfOptions,
};
use crate::draw::{self, Shape, ShapeKind};
use crate::engine::check_cancel;
use crate::error::{ErrorKind, RadialError, Result};
use crate::options::{CropRect, Rgb};
use pdfium_render::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn color(s: &Shape) -> PdfColor {
    let Rgb([r, g, b]) = Rgb::parse(&s.color).unwrap_or(Rgb([0, 0, 0]));
    PdfColor::new(r, g, b, 255)
}

/// Unit vectors (in user space) for "right" and "down" on the displayed page.
fn axes(p: &PdfPage) -> Result<((f32, f32), (f32, f32))> {
    let o = page_point(p, 0.0, 0.0)?;
    let r = page_point(p, 1.0, 0.0)?;
    let d = page_point(p, 0.0, 1.0)?;
    let unit = |a: (f32, f32)| {
        let (x, y) = (a.0 - o.0, a.1 - o.1);
        let l = (x * x + y * y).sqrt().max(f32::EPSILON);
        (x / l, y / l)
    };
    Ok((unit(r), unit(d)))
}

/// Characters Helvetica (WinAnsi) can show; anything else needs an embedded font.
pub(crate) fn winansi(s: &str) -> bool {
    s.chars()
        .all(|c| matches!(c, '\n' | '\r' | ' '..='~' | '\u{a0}'..='\u{ff}' | '€' | '–' | '—' | '‘' | '’' | '“' | '”' | '•' | '…'))
}

/// Draw `shapes` (each on its `page`) into the page content.
pub fn annotate(
    source: &Path,
    shapes: &[Shape],
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<String>)> {
    draw::validate_shapes(shapes)?;
    if shapes.is_empty() {
        return Err(RadialError::new(
            ErrorKind::InvalidOptions,
            "Add at least one annotation first.",
        ));
    }
    let pdfium = engine()?;
    let mut doc = open(pdfium, source)?;
    let pages = doc.pages().len() as u32;
    let before = char_counts(&doc);

    let texts: Vec<&str> = shapes
        .iter()
        .filter(|s| s.kind == ShapeKind::Text)
        .map(|s| s.text.as_str())
        .collect();
    let embedded = texts.iter().any(|t| !winansi(t));
    let font = if texts.is_empty() {
        None
    } else if embedded {
        Some(
            doc.fonts_mut()
                .load_true_type_from_bytes(draw::FONT_DATA, true)
                .map_err(|e| pdf_err("loading the font", e))?,
        )
    } else {
        Some(doc.fonts_mut().helvetica())
    };

    let mut by_page: BTreeMap<u32, Vec<&Shape>> = BTreeMap::new();
    for s in shapes {
        by_page.entry(s.page).or_default().push(s);
    }
    for (&index, list) in &by_page {
        check_cancel(cancel)?;
        let mut p = page(&doc, index)?;
        p.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        for s in list {
            draw_shape(&doc, &mut p, s, font)?;
        }
        p.regenerate_content()
            .map_err(|e| pdf_err("writing annotations", e))?;
    }
    let bytes = save(&doc)?;
    drop(doc);
    let bytes = finalize_doc(load_lopdf(&bytes)?, false, false)?;

    let after = super::verify(&bytes, pages)?;
    for (i, (&now, &was)) in after.iter().zip(&before).enumerate() {
        if now < was {
            return Err(RadialError::new(
                ErrorKind::EngineFailed,
                format!(
                    "Page {} lost text while annotating. Nothing was saved.",
                    i + 1
                ),
            ));
        }
    }
    let list: Vec<u32> = by_page.keys().copied().collect();
    let mut notes = vec![format!(
        "{} added to page{} {} as part of the page content.",
        plural(shapes.len(), "annotation", "annotations"),
        if list.len() == 1 { "" } else { "s" },
        super::page_list(&list)
    )];
    if embedded {
        notes.push("DejaVu Sans was embedded for the text, which adds about 0.7 MB.".into());
    }
    Ok((bytes, notes))
}

fn draw_shape<'a>(
    doc: &PdfDocument<'a>,
    p: &mut PdfPage<'a>,
    s: &Shape,
    font: Option<PdfFontToken>,
) -> Result<()> {
    let err = |e| pdf_err("drawing an annotation", e);
    let short = short_side(p);
    let c = color(s);
    let stroke = PdfPoints::new(((s.stroke * f64::from(short)) as f32).max(0.5));
    let (ax, ay) = page_point(p, s.x1, s.y1)?;
    let (bx, by) = page_point(p, s.x2, s.y2)?;
    let pt = PdfPoints::new;
    let objects = p.objects_mut();
    match s.kind {
        ShapeKind::Rect | ShapeKind::Ellipse => {
            let rect = PdfRect::new_from_values(ay.min(by), ax.min(bx), ay.max(by), ax.max(bx));
            // Filled shapes have no outline, as in the image renderer.
            let (line, fill) = if s.filled {
                (None, Some(c))
            } else {
                (Some(c), None)
            };
            if s.kind == ShapeKind::Rect {
                objects.create_path_object_rect(rect, line, Some(stroke), fill)
            } else {
                objects.create_path_object_ellipse(rect, line, Some(stroke), fill)
            }
            .map_err(err)?;
        }
        ShapeKind::Line | ShapeKind::Arrow => {
            let (dx, dy) = (bx - ax, by - ay);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 0.01 {
                return Ok(());
            }
            let (ux, uy) = (dx / len, dy / len);
            let (mut ex, mut ey) = (bx, by);
            if s.kind == ShapeKind::Arrow {
                // Same proportions as the image renderer.
                let head = (stroke.value * 4.0).max(short * 0.02).min(len * 0.6);
                let half = head * 0.55;
                let (hx, hy) = (bx - ux * head, by - uy * head);
                let mut tri = PdfPagePathObject::new(doc, pt(bx), pt(by), None, None, Some(c))
                    .map_err(err)?;
                tri.line_to(pt(hx - uy * half), pt(hy + ux * half))
                    .map_err(err)?;
                tri.line_to(pt(hx + uy * half), pt(hy - ux * half))
                    .map_err(err)?;
                tri.close_path().map_err(err)?;
                // Stop the shaft inside the head so its end doesn't poke out.
                ex = bx - ux * head * 0.8;
                ey = by - uy * head * 0.8;
                let mut line =
                    PdfPagePathObject::new(doc, pt(ax), pt(ay), Some(c), Some(stroke), None)
                        .map_err(err)?;
                line.line_to(pt(ex), pt(ey)).map_err(err)?;
                line.set_line_cap(PdfPageObjectLineCap::Round)
                    .map_err(err)?;
                objects.add_path_object(line).map_err(err)?;
                objects.add_path_object(tri).map_err(err)?;
            } else {
                let mut line =
                    PdfPagePathObject::new(doc, pt(ax), pt(ay), Some(c), Some(stroke), None)
                        .map_err(err)?;
                line.line_to(pt(ex), pt(ey)).map_err(err)?;
                line.set_line_cap(PdfPageObjectLineCap::Round)
                    .map_err(err)?;
                objects.add_path_object(line).map_err(err)?;
            }
        }
        ShapeKind::Text => {
            let Some(font) = font else { return Ok(()) };
            let size = ((s.size * f64::from(short)) as f32).max(1.0);
            let line_height = size * 1.15;
            let rotation = p.rotation().map_err(err)?.as_degrees();
            let (_, down) = axes(p)?;
            for (i, line) in s.text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                // Baseline sits one ascent (~0.8 em) below the line's top edge.
                let off = size * 0.8 + line_height * i as f32;
                let (ox, oy) = (ax + down.0 * off, ay + down.1 * off);
                let mut t =
                    PdfPageTextObject::new(doc, line, font, PdfPoints::new(size)).map_err(err)?;
                t.set_fill_color(c).map_err(err)?;
                if rotation != 0.0 {
                    t.rotate_counter_clockwise_degrees(rotation).map_err(err)?;
                }
                t.translate(pt(ox), pt(oy)).map_err(err)?;
                p.objects_mut().add_text_object(t).map_err(err)?;
            }
        }
    }
    Ok(())
}

/// Set the visible area (crop box) of one page or every page.
pub fn crop(
    source: &Path,
    rect: &CropRect,
    opts: &PdfOptions,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<String>)> {
    if !(rect.width > 0.0 && rect.height > 0.0) {
        return Err(RadialError::new(
            ErrorKind::InvalidOptions,
            "Draw the area to keep first.",
        ));
    }
    let pdfium = engine()?;
    let doc = open(pdfium, source)?;
    let pages = doc.pages().len() as u32;
    let targets: Vec<u32> = if opts.all_pages {
        (0..pages).collect()
    } else {
        vec![opts.page]
    };
    let mut expected = BTreeMap::new();
    for &i in &targets {
        check_cancel(cancel)?;
        let mut p = page(&doc, i)?;
        let r = page_rect(&p, rect)?;
        if r.width().value < 1.0 || r.height().value < 1.0 {
            return Err(RadialError::new(
                ErrorKind::InvalidOptions,
                "The crop area is too small.",
            ));
        }
        expected.insert(i, (r.width().value, r.height().value));
        p.boundaries_mut()
            .set_crop(r)
            .map_err(|e| pdf_err("setting the crop", e))?;
    }
    let bytes = save(&doc)?;
    drop(doc);
    let bytes = finalize_doc(load_lopdf(&bytes)?, false, false)?;
    super::verify(&bytes, pages)?;

    // The visible size of each cropped page must match the request.
    let check = pdfium
        .load_pdf_from_byte_slice(&bytes, None)
        .map_err(|e| pdf_err("verifying the result", e))?;
    for (&i, &(w, h)) in &expected {
        let p = page(&check, i)?;
        let b = p
            .boundaries()
            .crop()
            .map_err(|e| pdf_err("verifying the result", e))?
            .bounds;
        if (b.width().value - w).abs() > 0.5 || (b.height().value - h).abs() > 0.5 {
            return Err(RadialError::new(
                ErrorKind::EngineFailed,
                "The crop could not be verified. Nothing was saved.",
            ));
        }
    }
    drop(check);
    let notes = vec![
        if opts.all_pages {
            format!("All {pages} pages cropped.")
        } else {
            format!("Page {} cropped; other pages unchanged.", opts.page + 1)
        },
        "Content outside the crop is hidden, not deleted: PDF editors can restore it. Use Redact to remove sensitive content.".into(),
    ];
    Ok((bytes, notes))
}
