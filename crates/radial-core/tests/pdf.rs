//! Integration tests for the PDF engine on real files. These need the PDFium
//! library (`node scripts/fetch-pdfium.mjs`); they fail loudly without it.

use image::{DynamicImage, GenericImageView, Rgba};
use radial_core::detect::inspect;
use radial_core::draw::{Shape, ShapeKind};
use radial_core::metadata::MetadataEdit;
use radial_core::ops::{self, OperationId, OperationRequest};
use radial_core::options::{ConvertOptions, CropRect, Edits};
use radial_core::pdf::{self, PageRegion, PdfOptions};
use radial_core::{ErrorKind, Format, Registry};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn req(src: &Path, op: OperationId, dir: &Path, stem: &str) -> OperationRequest {
    OperationRequest {
        source: src.to_path_buf(),
        operation: op,
        target: None,
        dest_dir: dir.to_path_buf(),
        stem: stem.into(),
        options: ConvertOptions::default(),
        metadata: MetadataEdit::default(),
        max_dimension: None,
        pdf: PdfOptions::default(),
    }
}

fn run(r: &OperationRequest) -> radial_core::Result<ops::OperationOutcome> {
    ops::run(&Registry::new(), r, &AtomicBool::new(false), |_| {})
}

fn text_of(path: &Path) -> String {
    pdf::to_text(path, &AtomicBool::new(false))
        .map(|t| t.0)
        .unwrap_or_default()
}

fn page_text(path: &Path, index: u32) -> String {
    let pdfium = pdf::engine().unwrap();
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let p = doc.pages().get(index as i32).unwrap();
    pdf::page_text(&p).unwrap()
}

/// All decompressed stream contents and strings of a PDF, for byte searches.
fn raw_contents(path: &Path) -> Vec<u8> {
    let mut doc = lopdf::Document::load(path).unwrap();
    doc.decompress();
    let mut out = Vec::new();
    for o in doc.objects.values() {
        match o {
            lopdf::Object::Stream(s) => out.extend_from_slice(&s.content),
            other => out.extend_from_slice(format!("{other:?}").as_bytes()),
        }
    }
    out
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn assert_no_partials(dir: &Path) {
    for e in std::fs::read_dir(dir).unwrap() {
        let n = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(!n.starts_with(".radial-"), "temporary file left: {n}");
    }
}

#[test]
fn engine_is_available() {
    assert!(
        pdf::missing_reason().is_none(),
        "PDFium missing: run `node scripts/fetch-pdfium.mjs` ({:?})",
        pdf::missing_reason()
    );
}

#[test]
fn detects_pdfs_and_protection() {
    let info = inspect(&fixture("report.pdf")).unwrap();
    assert_eq!(info.format, Some(Format::Pdf));
    assert!(info.problem.is_none());
    assert_eq!(info.pages, Some(2));
    assert_eq!((info.width, info.height), (Some(595), Some(842)));
    // Page 2 is displayed rotated (landscape).
    let [w, h] = info.page_sizes[1];
    assert!(w > h, "{:?}", info.page_sizes);
    assert!(!info.restricted);

    let locked = inspect(&fixture("encrypted.pdf")).unwrap();
    let p = locked
        .problem
        .expect("password-protected PDF must be reported");
    assert!(p.message.contains("password"), "{}", p.message);

    let restricted = inspect(&fixture("restricted.pdf")).unwrap();
    assert!(restricted.problem.is_none());
    assert!(restricted.restricted);
    let ops = ops::operations_for(&Registry::new(), &restricted);
    for o in &ops {
        if o.id == OperationId::Metadata {
            assert!(o.available);
            assert!(o.options.is_empty(), "restricted PDFs are view-only");
        } else {
            assert!(!o.available, "{:?} must be unavailable", o.id);
        }
    }
}

#[test]
fn offers_pdf_operations_with_reasons() {
    let info = inspect(&fixture("report.pdf")).unwrap();
    let ops = ops::operations_for(&Registry::new(), &info);
    let get = |id| ops.iter().find(|o| o.id == id).unwrap();
    let targets: Vec<Format> = get(OperationId::Convert)
        .targets
        .iter()
        .map(|c| c.to)
        .collect();
    assert_eq!(targets, vec![Format::Png, Format::Jpeg, Format::Txt]);
    for id in [
        OperationId::Compress,
        OperationId::Crop,
        OperationId::Annotate,
        OperationId::Redact,
        OperationId::Metadata,
    ] {
        assert!(get(id).available, "{id:?}");
    }
    for id in [OperationId::Edit, OperationId::Background] {
        assert!(!get(id).available);
        assert!(get(id).reason.is_some());
    }
    // Every image format can become a PDF.
    let r = Registry::new();
    for f in Format::IMAGES {
        assert!(r.find(f, Format::Pdf).is_some_and(|c| c.available));
    }
}

#[test]
fn extracts_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = req(
        &fixture("report.pdf"),
        OperationId::Convert,
        dir.path(),
        "report",
    );
    r.target = Some(Format::Txt);
    let out = run(&r).unwrap();
    assert_eq!(out.format, Format::Txt);
    let text = std::fs::read_to_string(&out.output_path).unwrap();
    assert!(text.contains("ACME Corporation"));
    assert!(text.contains("ZEBRA-42"));
    assert!(out.output_path.extension().unwrap() == "txt");
}

#[test]
fn renders_pages_to_images_including_rotated_pages() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = req(
        &fixture("report.pdf"),
        OperationId::Convert,
        dir.path(),
        "p1",
    );
    r.target = Some(Format::Png);
    r.pdf.dpi = Some(72);
    let out = run(&r).unwrap();
    let img = image::open(&out.output_path).unwrap();
    assert_eq!(img.dimensions(), (595, 842));

    let mut r = req(
        &fixture("report.pdf"),
        OperationId::Convert,
        dir.path(),
        "p2",
    );
    r.target = Some(Format::Jpeg);
    r.pdf.page = 1;
    r.pdf.dpi = Some(100);
    r.options.quality = Some(80);
    let out = run(&r).unwrap();
    let img = image::open(&out.output_path).unwrap();
    // Page 2 has /Rotate 90, so it is landscape as displayed.
    assert_eq!(img.dimensions(), (1169, 827));

    let mut r = req(
        &fixture("report.pdf"),
        OperationId::Convert,
        dir.path(),
        "bad",
    );
    r.target = Some(Format::Png);
    r.pdf.page = 7;
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
    r.pdf.page = 0;
    r.pdf.dpi = Some(5000);
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
    assert_no_partials(dir.path());
}

#[test]
fn converts_images_to_pdf() {
    let dir = tempfile::tempdir().unwrap();
    // A transparent PNG keeps its alpha as a soft mask.
    let png = dir.path().join("logo.png");
    DynamicImage::ImageRgba8(image::RgbaImage::from_fn(300, 200, |x, _| {
        Rgba([200, 30, 30, if x < 150 { 255 } else { 0 }])
    }))
    .save(&png)
    .unwrap();
    let mut r = req(&png, OperationId::Convert, dir.path(), "logo");
    r.target = Some(Format::Pdf);
    let out = run(&r).unwrap();
    assert_eq!(out.format, Format::Pdf);
    let info = inspect(&out.output_path).unwrap();
    assert_eq!(info.pages, Some(1));
    assert!(info.width > info.height, "landscape image → landscape page");
    let page = pdf::render_page(&out.output_path, 0, 400)
        .unwrap()
        .to_rgb8();
    let c = page.get_pixel(page.width() / 2 - 20, page.height() / 2).0;
    assert!(c[0] > 150 && c[1] < 80, "image drawn: {c:?}");
    let t = page.get_pixel(page.width() / 2 + 20, page.height() / 2).0;
    assert_eq!(t, [255, 255, 255], "transparent half shows the page");

    // JPEG data is embedded byte-for-byte.
    let jpg = dir.path().join("photo.jpg");
    let photo = DynamicImage::ImageRgb8(image::RgbImage::from_fn(240, 320, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    }));
    photo.save(&jpg).unwrap();
    let mut r = req(&jpg, OperationId::Convert, dir.path(), "photo");
    r.target = Some(Format::Pdf);
    let out = run(&r).unwrap();
    let src = std::fs::read(&jpg).unwrap();
    let pdf_bytes = std::fs::read(&out.output_path).unwrap();
    assert!(pdf_bytes.windows(src.len()).any(|w| w == src.as_slice()));
    assert!(out.notes.iter().any(|n| n.contains("unchanged")));

    // An image-only PDF has no text to extract.
    let mut r = req(&out.output_path, OperationId::Convert, dir.path(), "scan");
    r.target = Some(Format::Txt);
    let e = run(&r).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnsupportedConversion);
    assert!(e.message.contains("OCR"));
    assert!(!dir.path().join("scan.txt").exists());
}

#[test]
fn compresses_pdf_and_keeps_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("report.pdf");
    let mut r = req(&src, OperationId::Compress, dir.path(), "small");
    r.options.quality = Some(40);
    let out = run(&r).unwrap();
    assert!(out.size_bytes < out.source_size_bytes, "{out:?}");
    assert_eq!(
        out.source_size_bytes,
        std::fs::metadata(&src).unwrap().len()
    );
    assert_eq!(inspect(&out.output_path).unwrap().pages, Some(2));
    assert_eq!(text_of(&out.output_path), text_of(&src));
    assert!(
        out.notes.iter().any(|n| n.contains("1 photo")),
        "{:?}",
        out.notes
    );

    // Compressing the result again can't shrink it further: nothing is saved.
    let mut again = req(
        &out.output_path,
        OperationId::Compress,
        dir.path(),
        "smaller",
    );
    again.options.quality = Some(95);
    assert_eq!(run(&again).unwrap_err().kind, ErrorKind::NoReduction);
    assert!(!dir.path().join("smaller.pdf").exists());
    assert_no_partials(dir.path());
}

#[test]
fn reads_and_edits_pdf_metadata() {
    let src = fixture("report.pdf");
    let report = pdf::read_metadata(&src).unwrap();
    let value = |r: &radial_core::metadata::MetadataReport, k: &str| {
        r.fields
            .iter()
            .find(|f| f.key == k)
            .and_then(|f| f.value.clone())
    };
    assert_eq!(value(&report, "title").as_deref(), Some("Quarterly report"));
    assert_eq!(value(&report, "author").as_deref(), Some("Old author"));
    assert!(report.can_edit);
    assert!(report
        .details
        .iter()
        .any(|d| d.label == "Pages" && d.value == "2"));

    let dir = tempfile::tempdir().unwrap();
    let mut r = req(&src, OperationId::Metadata, dir.path(), "meta");
    r.metadata.fields = BTreeMap::from([
        ("title".to_string(), "Rapport trimestriel – €".to_string()),
        ("author".to_string(), String::new()),
    ]);
    let out = run(&r).unwrap();
    let after = pdf::read_metadata(&out.output_path).unwrap();
    assert_eq!(
        value(&after, "title").as_deref(),
        Some("Rapport trimestriel – €")
    );
    assert_eq!(value(&after, "author"), None);
    assert_eq!(value(&after, "subject").as_deref(), Some("Account review"));
    assert_eq!(text_of(&out.output_path), text_of(&src));

    let mut r = req(&src, OperationId::Metadata, dir.path(), "stripped");
    r.metadata.strip_all = true;
    let out = run(&r).unwrap();
    let after = pdf::read_metadata(&out.output_path).unwrap();
    assert!(after.fields.iter().all(|f| f.value.is_none()));
    assert!(!contains(
        &std::fs::read(&out.output_path).unwrap(),
        "Old author"
    ));

    // Copyright isn't a PDF field.
    let mut r = req(&src, OperationId::Metadata, dir.path(), "bad");
    r.metadata.fields = BTreeMap::from([("copyright".to_string(), "x".to_string())]);
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
}

#[test]
fn crops_one_page_or_all() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("report.pdf");
    let top_half = CropRect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 0.5,
    };
    let mut r = req(&src, OperationId::Crop, dir.path(), "crop1");
    r.options.edits = Edits {
        crop: Some(top_half),
        ..Default::default()
    };
    let out = run(&r).unwrap();
    let pdfium = pdf::engine().unwrap();
    let doc = pdfium.load_pdf_from_file(&out.output_path, None).unwrap();
    let p0 = doc.pages().get(0).unwrap();
    assert!(
        (p0.height().value - 420.9).abs() < 1.0,
        "{}",
        p0.height().value
    );
    assert!((p0.width().value - 595.3).abs() < 1.0);
    let p1 = doc.pages().get(1).unwrap();
    assert!((p1.width().value - 841.9).abs() < 1.0, "page 2 unchanged");
    // The heading is in the kept half and still real text.
    assert!(pdf::page_text(&p0).unwrap().contains("Quarterly report"));
    drop(doc);

    let mut r = req(&src, OperationId::Crop, dir.path(), "crop-all");
    r.options.edits.crop = Some(top_half);
    r.pdf.all_pages = true;
    let out = run(&r).unwrap();
    let doc = pdfium.load_pdf_from_file(&out.output_path, None).unwrap();
    // Page 2 is displayed rotated; the crop applies to its displayed top half.
    let p1 = doc.pages().get(1).unwrap();
    assert!((p1.width().value - 841.9).abs() < 1.0);
    assert!(
        (p1.height().value - 297.6).abs() < 1.0,
        "{}",
        p1.height().value
    );
}

fn red_bbox(img: &image::RgbImage) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[0] > 180 && p.0[1] < 90 && p.0[2] < 90 {
            b = Some(match b {
                None => (x, y, x, y),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
            });
        }
    }
    b
}

#[test]
fn annotates_pages_with_vector_shapes_and_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("report.pdf");
    let shape = |kind, page, x1, y1, x2, y2| Shape {
        kind,
        x1,
        y1,
        x2,
        y2,
        color: "#e01010".into(),
        stroke: 0.006,
        filled: false,
        text: String::new(),
        size: 0.04,
        page,
    };
    let mut boxed = shape(ShapeKind::Rect, 0, 0.6, 0.7, 0.9, 0.8);
    boxed.filled = true;
    let mut label = shape(ShapeKind::Text, 1, 0.5, 0.6, 0.0, 0.0);
    label.text = "Approved".into();
    let mut r = req(&src, OperationId::Annotate, dir.path(), "notes");
    r.options.annotations = vec![
        boxed,
        shape(ShapeKind::Arrow, 0, 0.1, 0.9, 0.4, 0.75),
        label,
    ];
    let out = run(&r).unwrap();

    let p0 = pdf::render_page(&out.output_path, 0, 842)
        .unwrap()
        .to_rgb8();
    let (w, h) = (p0.width() as f64, p0.height() as f64);
    let c = p0.get_pixel((0.75 * w) as u32, (0.75 * h) as u32).0;
    assert!(
        c[0] > 180 && c[1] < 90,
        "filled box drawn at its place: {c:?}"
    );

    // Text on the rotated page reads upright where it was placed.
    let p1 = pdf::render_page(&out.output_path, 1, 842)
        .unwrap()
        .to_rgb8();
    let (x0, y0, x1, y1) = red_bbox(&p1).expect("text drawn");
    let (w, h) = (p1.width() as f64, p1.height() as f64);
    assert!(
        x1 - x0 > 2 * (y1 - y0),
        "horizontal text: {:?}",
        (x0, y0, x1, y1)
    );
    assert!((f64::from(x0) / w - 0.5).abs() < 0.02, "x {x0}");
    assert!((f64::from(y0) / h - 0.6).abs() < 0.03, "y {y0}");
    assert!(page_text(&out.output_path, 1).contains("Approved"));
    // Existing text is untouched.
    assert!(page_text(&out.output_path, 0).contains("ACME Corporation"));

    let mut bad = req(&src, OperationId::Annotate, dir.path(), "bad");
    bad.options.annotations = vec![shape(ShapeKind::Rect, 9, 0.1, 0.1, 0.2, 0.2)];
    assert_eq!(run(&bad).unwrap_err().kind, ErrorKind::InvalidOptions);
}

#[test]
fn redacts_areas_genuinely() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("report.pdf");
    assert!(contains(&raw_contents(&src), "DE89"));
    let mut r = req(&src, OperationId::Redact, dir.path(), "redacted");
    // The IBAN line: baseline 150pt below the top, 12pt Helvetica.
    r.pdf.regions = vec![PageRegion {
        page: 0,
        rect: CropRect {
            x: 0.08,
            y: 0.162,
            width: 0.6,
            height: 0.026,
        },
    }];
    let out = run(&r).unwrap();
    let path = &out.output_path;

    let raw = raw_contents(path);
    for gone in ["DE89", "IBAN", "ACME", "Quarterly report", "Old author"] {
        assert!(!contains(&raw, gone), "“{gone}” still in the file");
    }
    assert_eq!(page_text(path, 0).trim(), "", "redacted page has no text");
    assert!(
        page_text(path, 1).contains("ZEBRA-42"),
        "other pages unchanged"
    );

    // The area is black; the rest of the page still shows its content.
    let p0 = pdf::render_page(path, 0, 842).unwrap().to_rgb8();
    let (w, h) = (p0.width() as f64, p0.height() as f64);
    let at = |x: f64, y: f64| p0.get_pixel((x * w) as u32, (y * h) as u32).0;
    assert_eq!(at(0.3, 0.175), [0, 0, 0]);
    let heading_dark = (0..p0.width())
        .map(|x| p0.get_pixel(x, (0.103 * h) as u32).0[0])
        .min()
        .unwrap();
    assert!(heading_dark < 100, "heading still visible as pixels");
    assert!(out.notes.iter().any(|n| n.contains("page 1")));
    assert_no_partials(dir.path());
}

#[test]
fn redacts_search_terms_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("report.pdf");
    let mut r = req(&src, OperationId::Redact, dir.path(), "terms");
    r.pdf.terms = vec!["zebra-42".into(), "  ".into()];
    let out = run(&r).unwrap();
    let path = &out.output_path;
    assert!(!contains(&raw_contents(path), "ZEBRA"));
    assert_eq!(page_text(path, 1).trim(), "");
    assert!(page_text(path, 0).contains("ACME Corporation"));
    // The rotated page keeps its displayed orientation and size.
    let pdfium = pdf::engine().unwrap();
    let doc = pdfium.load_pdf_from_file(path, None).unwrap();
    let p1 = doc.pages().get(1).unwrap();
    assert!((p1.width().value - 841.9).abs() < 1.0);
    drop(doc);
    let img = pdf::render_page(path, 1, 842).unwrap().to_rgb8();
    let ink = img.pixels().filter(|p| p.0[0] < 60).count();
    assert!(ink > 500, "black box and remaining text rendered");

    let mut none = req(&src, OperationId::Redact, dir.path(), "none");
    none.pdf.terms = vec!["not in this file".into()];
    let e = run(&none).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidOptions);
    assert!(!dir.path().join("none.pdf").exists());

    let empty = req(&src, OperationId::Redact, dir.path(), "empty");
    assert_eq!(run(&empty).unwrap_err().kind, ErrorKind::InvalidOptions);
}

#[test]
fn refuses_unavailable_pdf_operations() {
    let dir = tempfile::tempdir().unwrap();
    let r = req(
        &fixture("report.pdf"),
        OperationId::Background,
        dir.path(),
        "x",
    );
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::UnsupportedConversion);
    let r = req(
        &fixture("restricted.pdf"),
        OperationId::Compress,
        dir.path(),
        "x",
    );
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::UnsupportedConversion);
}

#[test]
fn redaction_refuses_text_it_cannot_remove() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("bookmarked.pdf");
    // Only in a bookmark title: explained, nothing written.
    let mut r = req(&src, OperationId::Redact, dir.path(), "a");
    r.pdf.terms = vec!["nightjar".into()];
    let e = run(&r).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnsupportedConversion);
    assert!(e.message.contains("bookmark"), "{}", e.message);
    // On a page *and* in a bookmark: the page is redacted, then verification
    // finds the bookmark copy and nothing is saved.
    let mut r = req(&src, OperationId::Redact, dir.path(), "b");
    r.pdf.terms = vec!["VAULT-7731".into()];
    let e = run(&r).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineFailed);
    assert!(
        e.message.contains("outside the page content"),
        "{}",
        e.message
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
