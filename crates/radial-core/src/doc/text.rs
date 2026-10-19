//! Plain text files: detection and conversion to PDF.

use crate::draw::FONT_DATA;
use crate::engine::check_cancel;
use crate::error::{ErrorKind, RadialError, Result};
use crate::pdf::{self, pdf_err, plural};
use ab_glyph::{Font, FontRef};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Largest text file RADIAL will open.
pub const MAX_TEXT_BYTES: u64 = 20_000_000;

const PAGE: (f32, f32) = (595.276, 841.89);
const MARGIN: f32 = 56.0;
const SIZE: f32 = 10.0;
const LEADING: f32 = 13.0;

/// Decode text: UTF-8 (with or without BOM) or UTF-16 with a BOM. Returns
/// `None` for binary content (NUL or other control characters).
pub fn decode(bytes: &[u8]) -> Option<String> {
    let s = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8(rest.to_vec()).ok()?
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (units, tail) = rest.as_chunks::<2>();
        if !tail.is_empty() {
            return None;
        }
        String::from_utf16(
            &units
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect::<Vec<_>>(),
        )
        .ok()?
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (units, tail) = rest.as_chunks::<2>();
        if !tail.is_empty() {
            return None;
        }
        String::from_utf16(
            &units
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect::<Vec<_>>(),
        )
        .ok()?
    } else {
        String::from_utf8(bytes.to_vec()).ok()?
    };
    s.chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t' | '\x0c'))
        .then_some(s)
}

/// Read and decode a text file, enforcing the size limit.
pub fn read(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    if meta.len() > MAX_TEXT_BYTES {
        return Err(RadialError::new(
            ErrorKind::TooLarge,
            format!(
                "Text files larger than {} MB are not supported.",
                MAX_TEXT_BYTES / 1_000_000
            ),
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    decode(&bytes).ok_or_else(|| {
        RadialError::new(
            ErrorKind::InvalidFile,
            "This file isn't plain text (it contains binary data or an unsupported encoding).",
        )
    })
}

/// Width of text in points, for wrapping.
enum Metrics<'a> {
    /// Courier: every glyph is 0.6 em.
    Mono,
    Font(Box<FontRef<'a>>),
}

impl Metrics<'_> {
    fn width(&self, s: &str) -> f32 {
        match self {
            Metrics::Mono => s.chars().count() as f32 * 0.6 * SIZE,
            Metrics::Font(f) => {
                let em = f.units_per_em().unwrap_or(2048.0);
                s.chars()
                    .map(|c| f.h_advance_unscaled(f.glyph_id(c)))
                    .sum::<f32>()
                    / em
                    * SIZE
            }
        }
    }
}

/// Lines of the laid-out text; `None` marks a forced page break (form feed).
fn layout(text: &str, m: &Metrics, max: f32) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for (i, page) in text.split('\x0c').enumerate() {
        if i > 0 {
            out.push(None);
        }
        for raw in page.split('\n') {
            // Tabs to the next multiple of four columns.
            let mut line = String::new();
            for c in raw.chars() {
                if c == '\t' {
                    let n = 4 - line.chars().count() % 4;
                    line.extend(std::iter::repeat_n(' ', n));
                } else {
                    line.push(c);
                }
            }
            let line = line.trim_end();
            if m.width(line) <= max {
                out.push(Some(line.to_string()));
                continue;
            }
            // Greedy wrap at spaces; break words longer than a line.
            let mut cur = String::new();
            for word in line.split_inclusive(' ') {
                if m.width(&cur) + m.width(word.trim_end()) <= max {
                    cur.push_str(word);
                    continue;
                }
                if !cur.trim().is_empty() {
                    out.push(Some(cur.trim_end().to_string()));
                    cur.clear();
                }
                for c in word.chars() {
                    let mut probe = cur.clone();
                    probe.push(c);
                    if m.width(probe.trim_end()) > max && !cur.is_empty() {
                        out.push(Some(cur.trim_end().to_string()));
                        cur.clear();
                    }
                    cur.push(c);
                }
            }
            if !cur.trim().is_empty() {
                out.push(Some(cur.trim_end().to_string()));
            }
        }
    }
    out
}

/// Typeset a text file on A4 pages.
pub fn to_pdf(path: &Path, cancel: &AtomicBool) -> Result<(Vec<u8>, Vec<String>)> {
    let text = read(path)?;
    if text.trim().is_empty() {
        return Err(RadialError::new(
            ErrorKind::InvalidFile,
            "The text file is empty, so there is nothing to convert.",
        ));
    }
    let mono = pdf::annotate::winansi(&text.replace(['\t', '\x0c'], " "));
    let metrics = if mono {
        Metrics::Mono
    } else {
        Metrics::Font(Box::new(FontRef::try_from_slice(FONT_DATA).map_err(
            |_| RadialError::new(ErrorKind::Internal, "The bundled font could not be read."),
        )?))
    };
    let lines = layout(&text, &metrics, PAGE.0 - 2.0 * MARGIN);
    let per_page = ((PAGE.1 - 2.0 * MARGIN) / LEADING).floor() as usize;

    let pdfium = pdf::engine()?;
    let mut doc = pdfium
        .create_new_pdf()
        .map_err(|e| pdf_err("creating the document", e))?;
    let font = if mono {
        doc.fonts_mut().courier()
    } else {
        doc.fonts_mut()
            .load_true_type_from_bytes(FONT_DATA, true)
            .map_err(|e| pdf_err("loading the font", e))?
    };
    let mut pages: Vec<Vec<String>> = vec![Vec::new()];
    for l in lines {
        match l {
            None => pages.push(Vec::new()),
            Some(s) => {
                if pages.last().is_some_and(|p| p.len() >= per_page) {
                    pages.push(Vec::new());
                }
                if let Some(p) = pages.last_mut() {
                    p.push(s);
                }
            }
        }
    }
    if pages.len() as u32 > pdf::MAX_PAGES {
        return Err(RadialError::new(
            ErrorKind::TooLarge,
            format!("The text would need more than {} pages.", pdf::MAX_PAGES),
        ));
    }
    for page_lines in &pages {
        check_cancel(cancel)?;
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .map_err(|e| pdf_err("adding a page", e))?;
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        for (i, line) in page_lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            let y = PAGE.1 - MARGIN - SIZE - LEADING * i as f32;
            let mut t = PdfPageTextObject::new(&doc, line, font, PdfPoints::new(SIZE))
                .map_err(|e| pdf_err("writing text", e))?;
            t.translate(PdfPoints::new(MARGIN), PdfPoints::new(y))
                .map_err(|e| pdf_err("writing text", e))?;
            page.objects_mut()
                .add_text_object(t)
                .map_err(|e| pdf_err("writing text", e))?;
        }
        page.regenerate_content()
            .map_err(|e| pdf_err("writing a page", e))?;
    }
    let bytes = pdf::save(&doc)?;
    drop(doc);
    let bytes = pdf::finalize_doc(pdf::load_lopdf(&bytes)?, false, true)?;
    verify(&bytes, pages.len() as u32, &text)?;

    let mut notes = vec![format!(
        "{} on A4 in {}.",
        plural(pages.len(), "page", "pages"),
        if mono {
            "Courier (monospaced, so columns stay aligned)"
        } else {
            "DejaVu Sans, embedded for the characters this text uses (about 0.7 MB)"
        }
    )];
    notes.push("Long lines are wrapped; form feeds start a new page.".into());
    Ok((bytes, notes))
}

/// The PDF must contain exactly the source's characters (ignoring spacing).
fn verify(data: &[u8], pages: u32, source: &str) -> Result<()> {
    pdf::verify_pages(data, pages)?;
    let pdfium = pdf::engine()?;
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| pdf_err("verifying the result", e))?;
    let mut got = String::new();
    for p in doc.pages().iter() {
        got.push_str(&pdf::page_text(&p)?);
    }
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if squash(&got) != squash(source) {
        return Err(RadialError::new(
            ErrorKind::EngineFailed,
            "The PDF's text didn't match the file, so nothing was saved.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_text_and_rejects_binary() {
        assert_eq!(decode(b"\xEF\xBB\xBFhi\n").as_deref(), Some("hi\n"));
        assert_eq!(
            decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]).as_deref(),
            Some("hi")
        );
        assert_eq!(decode("café\tx".as_bytes()).as_deref(), Some("café\tx"));
        assert!(decode(b"PK\x03\x04\x00\x00").is_none());
        assert!(decode(&[0xC3, 0x28]).is_none());
    }

    #[test]
    fn wraps_long_lines_and_honours_form_feeds() {
        let long = "word ".repeat(40);
        let lines = layout(&format!("{long}\x0cnext\ta"), &Metrics::Mono, 60.0);
        assert!(lines.iter().flatten().all(|l| l.chars().count() <= 10));
        assert!(lines.contains(&None));
        assert_eq!(lines.last().unwrap().as_deref(), Some("next    a"));
    }
}
