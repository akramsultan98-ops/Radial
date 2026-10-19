//! The PDF engine.
//!
//! - [PDFium](https://pdfium.googlesource.com/pdfium/) (BSD-3-Clause, the
//!   engine inside Chrome) renders pages, extracts and searches text, and
//!   edits page content. It is a shared library loaded at runtime from a
//!   pinned build bundled with the app (see `scripts/fetch-pdfium.mjs`).
//! - [lopdf](https://crates.io/crates/lopdf) (MIT) edits the document
//!   structure: metadata, stream compression and removal of unreferenced
//!   objects, so nothing deleted by an operation lingers in the file.
//!
//! Page positions from the UI are fractions (0–1) of the page *as displayed*
//! (after its /Rotate and within its crop box). [`page_rect`] and
//! [`page_point`] map them to PDF user space with PDFium's own device
//! transform, so rotated and offset pages are handled exactly as rendered.

mod annotate;
mod compress;
mod convert;
mod meta;
mod redact;

pub use annotate::{annotate, crop};
pub use compress::compress;
pub use convert::{image_to_pdf, to_image, to_text};
pub use meta::{editable_keys as meta_keys, read_metadata, verify_metadata, write_metadata};
pub use redact::redact;

use crate::error::{ErrorKind, RadialError, Result};
use crate::options::CropRect;
use image::DynamicImage;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Most pages RADIAL will open in one document.
pub const MAX_PAGES: u32 = 5000;
pub const DEFAULT_DPI: u32 = 150;
pub const MIN_DPI: u32 = 36;
pub const MAX_DPI: u32 = 600;
/// Resolution used when a redacted page is replaced by an image.
pub const REDACT_DPI: u32 = 200;
/// Largest rendered edge, in pixels.
const MAX_RENDER_EDGE: u32 = 12_000;
/// Size of the virtual device used for fraction → point mapping.
const DEVICE: i32 = 10_000;

static SEARCH_DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static ENGINE: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

/// Add folders to search for the PDFium library (e.g. the app's resource
/// folder). Must be called before the engine is first used.
pub fn configure(dirs: impl IntoIterator<Item = PathBuf>) {
    if let Ok(mut d) = SEARCH_DIRS.lock() {
        d.extend(dirs);
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("RADIAL_PDFIUM") {
        v.push(PathBuf::from(p));
    }
    if let Ok(d) = SEARCH_DIRS.lock() {
        v.extend(d.iter().cloned());
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        v.push(exe_dir.join("pdfium"));
        v.push(exe_dir.join("resources").join("pdfium"));
        v.push(exe_dir);
    }
    // Development builds and tests use the copy fetched into the source tree.
    #[cfg(debug_assertions)]
    v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/pdfium"));
    v
}

fn bind() -> std::result::Result<Pdfium, String> {
    let mut last = None;
    for c in candidates() {
        let lib = if c.is_file() {
            c
        } else {
            Pdfium::pdfium_platform_library_name_at_path(&c)
        };
        if !lib.is_file() {
            continue;
        }
        match Pdfium::bind_to_library(&lib) {
            Ok(b) => return Ok(Pdfium::new(b)),
            Err(e) => last = Some(format!("{}: {e:?}", lib.display())),
        }
    }
    Err(match last {
        Some(e) => format!("The PDF engine could not be loaded ({e})."),
        None => "The PDF engine (PDFium) was not found. Reinstall RADIAL to restore it.".into(),
    })
}

/// The loaded engine, or `EngineMissing`.
pub fn engine() -> Result<&'static Pdfium> {
    ENGINE
        .get_or_init(bind)
        .as_ref()
        .map_err(|e| RadialError::new(ErrorKind::EngineMissing, e.clone()))
}

/// `None` when the engine is available, otherwise why not.
pub fn missing_reason() -> Option<String> {
    engine().err().map(|e| e.message)
}

pub(crate) fn pdf_err(doing: &str, e: PdfiumError) -> RadialError {
    match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            RadialError::new(
                ErrorKind::UnsupportedFormat,
                "This PDF is password-protected. Remove the password in your PDF reader first.",
            )
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            RadialError::new(
                ErrorKind::CorruptFile,
                "The PDF is damaged or not a valid PDF.",
            )
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            RadialError::new(
                ErrorKind::UnsupportedFormat,
                "This PDF uses an unsupported security handler.",
            )
        }
        other => RadialError::new(
            ErrorKind::EngineFailed,
            format!("The PDF engine failed while {doing} ({other:?})."),
        ),
    }
}

pub(crate) fn open<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<PdfDocument<'a>> {
    pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| pdf_err("opening the file", e))
}

/// What detection needs to know about a PDF.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfInfo {
    pub pages: u32,
    /// First page as displayed, in points.
    pub width: f32,
    pub height: f32,
    /// Has an owner password / permission restrictions.
    pub restricted: bool,
    /// Carries a digital signature.
    pub signed: bool,
    /// Displayed size of every page, in points.
    pub page_sizes: Vec<[f32; 2]>,
}

pub fn inspect(path: &Path) -> Result<PdfInfo> {
    let pdfium = engine()?;
    let doc = open(pdfium, path)?;
    let pages = doc.pages().len() as u32;
    if pages == 0 {
        return Err(RadialError::new(
            ErrorKind::CorruptFile,
            "The PDF has no pages.",
        ));
    }
    if pages > MAX_PAGES {
        return Err(RadialError::new(
            ErrorKind::TooLarge,
            format!("PDFs with more than {MAX_PAGES} pages are not supported."),
        ));
    }
    let first = doc
        .pages()
        .get(0)
        .map_err(|e| pdf_err("reading the first page", e))?;
    let restricted = !matches!(
        doc.permissions().security_handler_revision(),
        Ok(PdfSecurityHandlerRevision::Unprotected)
    );
    let page_sizes = doc
        .pages()
        .page_sizes()
        .map_err(|e| pdf_err("reading page sizes", e))?
        .iter()
        .map(|r| [r.width().value, r.height().value])
        .collect();
    Ok(PdfInfo {
        pages,
        width: first.width().value,
        height: first.height().value,
        page_sizes,
        restricted,
        signed: !doc.signatures().is_empty(),
    })
}

pub(crate) fn page<'a>(doc: &PdfDocument<'a>, index: u32) -> Result<PdfPage<'a>> {
    let n = doc.pages().len() as u32;
    if index >= n {
        return Err(RadialError::new(
            ErrorKind::InvalidOptions,
            format!("Page {} doesn't exist (the PDF has {n}).", index + 1),
        ));
    }
    doc.pages()
        .get(index as i32)
        .map_err(|e| pdf_err("reading a page", e))
}

/// Pixel size of a page rendered at `dpi`, capped to [`MAX_RENDER_EDGE`].
pub(crate) fn pixel_size(page: &PdfPage, dpi: u32) -> (i32, i32) {
    let scale = dpi as f32 / 72.0;
    let (w, h) = (page.width().value * scale, page.height().value * scale);
    let cap = (MAX_RENDER_EDGE as f32 / w.max(h)).min(1.0);
    (
        ((w * cap).round() as i32).max(1),
        ((h * cap).round() as i32).max(1),
    )
}

pub(crate) fn render_config(w: i32, h: i32) -> PdfRenderConfig {
    PdfRenderConfig::new()
        .set_fixed_size(w, h)
        .render_annotations(true)
        .render_form_data(true)
        .use_print_quality(true)
}

/// Render a page (as displayed) to an opaque RGB image of exactly `w`×`h`.
pub(crate) fn render(page: &PdfPage, w: i32, h: i32) -> Result<DynamicImage> {
    let bitmap = page
        .render_with_config(&render_config(w, h))
        .map_err(|e| pdf_err("rendering a page", e))?;
    let img = bitmap
        .as_image()
        .map_err(|e| pdf_err("rendering a page", e))?;
    Ok(DynamicImage::ImageRgb8(img.to_rgb8()))
}

/// Render page `index` so its longer side is at most `max_dim` pixels.
pub fn render_page(path: &Path, index: u32, max_dim: u32) -> Result<DynamicImage> {
    let pdfium = engine()?;
    let doc = open(pdfium, path)?;
    let page = page(&doc, index)?;
    let (pw, ph) = (page.width().value, page.height().value);
    let scale = max_dim as f32 / pw.max(ph);
    let w = ((pw * scale).round() as i32).max(1);
    let h = ((ph * scale).round() as i32).max(1);
    render(&page, w, h)
}

fn device(page: &PdfPage) -> (PdfRenderConfig, i32, i32) {
    let (pw, ph) = (page.width().value, page.height().value);
    let (w, h) = if pw >= ph {
        (DEVICE, ((DEVICE as f32) * ph / pw).round().max(1.0) as i32)
    } else {
        (((DEVICE as f32) * pw / ph).round().max(1.0) as i32, DEVICE)
    };
    (PdfRenderConfig::new().set_fixed_size(w, h), w, h)
}

/// A display-space fraction point → PDF user space.
pub(crate) fn page_point(page: &PdfPage, x: f64, y: f64) -> Result<(f32, f32)> {
    let (cfg, w, h) = device(page);
    let px = (x.clamp(0.0, 1.0) * f64::from(w)).round() as i32;
    let py = (y.clamp(0.0, 1.0) * f64::from(h)).round() as i32;
    let (a, b) = page
        .pixels_to_points(px, py, &cfg)
        .map_err(|e| pdf_err("mapping a position", e))?;
    Ok((a.value, b.value))
}

/// A display-space fraction rectangle → PDF user-space rectangle.
pub(crate) fn page_rect(page: &PdfPage, r: &CropRect) -> Result<PdfRect> {
    let (x1, y1) = page_point(page, r.x, r.y)?;
    let (x2, y2) = page_point(page, r.x + r.width, r.y + r.height)?;
    Ok(PdfRect::new_from_values(
        y1.min(y2),
        x1.min(x2),
        y1.max(y2),
        x1.max(x2),
    ))
}

/// Displayed length (in points) of the page's shorter side.
pub(crate) fn short_side(page: &PdfPage) -> f32 {
    page.width().value.min(page.height().value)
}

/// One area on one page (0-based), as fractions of the displayed page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRegion {
    pub page: u32,
    #[serde(flatten)]
    pub rect: CropRect,
}

/// PDF-specific parts of an operation request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PdfOptions {
    /// The page shown in the editor (0-based): Crop's reference page and
    /// the page exported by PDF → image.
    pub page: u32,
    /// Crop: apply to every page instead of only `page`.
    pub all_pages: bool,
    /// Redact: areas to remove.
    pub regions: Vec<PageRegion>,
    /// Redact: words or phrases to remove wherever they occur.
    pub terms: Vec<String>,
    /// PDF → image resolution.
    pub dpi: Option<u32>,
}

pub const MAX_TERMS: usize = 50;
pub const MAX_TERM_CHARS: usize = 200;

impl PdfOptions {
    pub fn dpi(&self) -> Result<u32> {
        let d = self.dpi.unwrap_or(DEFAULT_DPI);
        if !(MIN_DPI..=MAX_DPI).contains(&d) {
            return Err(RadialError::new(
                ErrorKind::InvalidOptions,
                format!("Resolution must be between {MIN_DPI} and {MAX_DPI} DPI."),
            ));
        }
        Ok(d)
    }

    /// Trimmed, non-empty search terms.
    pub fn clean_terms(&self) -> Result<Vec<String>> {
        let terms: Vec<String> = self
            .terms
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if terms.len() > MAX_TERMS {
            return Err(RadialError::new(
                ErrorKind::InvalidOptions,
                format!("Too many search terms (the limit is {MAX_TERMS})."),
            ));
        }
        if terms
            .iter()
            .any(|t| t.chars().count() > MAX_TERM_CHARS || t.chars().any(char::is_control))
        {
            return Err(RadialError::new(
                ErrorKind::InvalidOptions,
                "A search term is too long or contains control characters.",
            ));
        }
        Ok(terms)
    }
}

pub(crate) fn load_lopdf(data: &[u8]) -> Result<lopdf::Document> {
    lopdf::Document::load_mem(data).map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("The PDF's structure could not be read ({e})."),
        )
    })
}

/// Optionally remove document-level metadata (Info dictionary and XMP), drop
/// every object no longer referenced, and serialise a fresh, non-incremental
/// file — so nothing an operation removed lingers in the output.
pub(crate) fn finalize_doc(
    mut doc: lopdf::Document,
    strip_metadata: bool,
    compact: bool,
) -> Result<Vec<u8>> {
    if strip_metadata {
        meta::strip_document_metadata(&mut doc);
    }
    doc.prune_objects();
    save_lopdf(&mut doc, compact)
}

pub(crate) fn save_lopdf(doc: &mut lopdf::Document, compact: bool) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let res = if compact {
        doc.compress();
        let opts = lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .compression_level(9)
            .build();
        doc.save_with_options(&mut out, opts)
    } else {
        doc.save_to(&mut out)
    };
    res.map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("The PDF could not be written ({e})."),
        )
    })?;
    Ok(out)
}

/// Check a produced PDF opens with the expected page count and every page
/// renders. Returns the per-page character counts.
pub(crate) fn verify(data: &[u8], pages: u32) -> Result<Vec<usize>> {
    let pdfium = engine()?;
    let fail = |m: &str| RadialError::new(ErrorKind::EngineFailed, m.to_string());
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|_| fail("The result is not a valid PDF."))?;
    if doc.pages().len() as u32 != pages {
        return Err(fail("The result has the wrong number of pages."));
    }
    let mut chars = Vec::new();
    for p in doc.pages().iter() {
        p.render_with_config(&PdfRenderConfig::new().set_target_width(48))
            .map_err(|_| fail("A page of the result could not be rendered."))?;
        chars.push(p.text().map(|t| t.chars().len()).unwrap_or(0));
    }
    Ok(chars)
}

/// Check a produced PDF opens with `pages` pages that all render.
pub fn verify_pages(data: &[u8], pages: u32) -> Result<()> {
    verify(data, pages).map(|_| ())
}

/// Every character on a page in content order, with PDFium's generated
/// line breaks. (`PdfPageText::all` clips to an unrotated page box, which
/// loses text on rotated or cropped pages.)
pub fn page_text(page: &PdfPage) -> Result<String> {
    let text = page.text().map_err(|e| pdf_err("reading text", e))?;
    let mut s: String = text
        .chars()
        .iter()
        .filter_map(|c| c.unicode_char())
        .collect();
    s = s.replace("\r\n", "\n").replace('\r', "\n");
    Ok(s)
}

/// Per-page character counts of a source document.
pub(crate) fn char_counts(doc: &PdfDocument) -> Vec<usize> {
    doc.pages()
        .iter()
        .map(|p| p.text().map(|t| t.chars().len()).unwrap_or(0))
        .collect()
}

/// Serialise a pdfium document.
pub(crate) fn save(doc: &PdfDocument) -> Result<Vec<u8>> {
    doc.save_to_bytes()
        .map_err(|e| pdf_err("saving the document", e))
}

/// Pages in a human list: "1, 3 and 4".
pub(crate) fn page_list(pages: &[u32]) -> String {
    let v: Vec<String> = pages.iter().map(|p| (p + 1).to_string()).collect();
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
