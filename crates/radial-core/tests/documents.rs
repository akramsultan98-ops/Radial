//! Integration tests for text files and Word documents on real files.

use radial_core::detect::inspect;
use radial_core::doc::docx;
use radial_core::metadata::MetadataEdit;
use radial_core::ops::{self, OperationId, OperationRequest};
use radial_core::options::ConvertOptions;
use radial_core::pdf::{self, PdfOptions};
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

fn pdf_text(path: &Path) -> String {
    pdf::to_text(path, &AtomicBool::new(false)).unwrap().0
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn detects_documents_by_content() {
    let d = inspect(&fixture("memo.docx")).unwrap();
    assert_eq!(d.format, Some(Format::Docx));
    assert!(d.problem.is_none());
    let t = inspect(&fixture("notes.txt")).unwrap();
    assert_eq!(t.format, Some(Format::Txt));

    let dir = tempfile::tempdir().unwrap();
    // A .docx that is really a plain ZIP is not a Word document.
    let zip = dir.path().join("fake.docx");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        w.start_file("a.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, b"hi").unwrap();
        w.finish().unwrap();
    }
    let z = inspect(&zip).unwrap();
    assert_eq!(z.format, None);
    assert!(z.problem.is_some());
    // Binary content with a .txt name is not text.
    let bin = dir.path().join("data.txt");
    std::fs::write(&bin, [0u8, 159, 146, 150, 1, 2, 3]).unwrap();
    assert!(inspect(&bin).unwrap().problem.is_some());
    // Text without a text extension is not guessed.
    let other = dir.path().join("readme");
    std::fs::write(&other, "hello").unwrap();
    assert!(inspect(&other).unwrap().problem.is_some());
}

#[test]
fn document_operations_are_honest() {
    let reg = Registry::new();
    let ops = ops::operations_for(&reg, &inspect(&fixture("memo.docx")).unwrap());
    let get = |id| ops.iter().find(|o| o.id == id).unwrap();
    let targets: Vec<_> = get(OperationId::Convert)
        .targets
        .iter()
        .map(|c| c.to)
        .collect();
    assert_eq!(targets, vec![Format::Txt], "no DOCX → PDF");
    assert!(get(OperationId::Metadata).available);
    for id in [
        OperationId::Compress,
        OperationId::Redact,
        OperationId::Annotate,
    ] {
        assert!(!get(id).available && get(id).reason.is_some());
    }
    let ops = ops::operations_for(&reg, &inspect(&fixture("notes.txt")).unwrap());
    let convert = ops.iter().find(|o| o.id == OperationId::Convert).unwrap();
    assert_eq!(
        convert.targets.iter().map(|c| c.to).collect::<Vec<_>>(),
        vec![Format::Pdf]
    );
    assert!(ops.iter().filter(|o| o.available).count() == 1);
}

#[test]
fn word_to_text_keeps_words_and_paragraphs() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = req(
        &fixture("memo.docx"),
        OperationId::Convert,
        dir.path(),
        "memo",
    );
    r.target = Some(Format::Txt);
    let out = run(&r).unwrap();
    let text = std::fs::read_to_string(&out.output_path).unwrap();
    assert!(text.starts_with("Client memo\n"));
    assert!(text.contains("Account: ACME Corporation\tTier 1\n"));
    assert!(text.contains("Renewal due 31 March — budget & scope agreed.\n"));
    assert!(text.contains("Contact\nJane Doe\n"));
}

#[test]
fn text_to_pdf_typesets_every_character() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("notes.txt");
    let mut r = req(&src, OperationId::Convert, dir.path(), "notes");
    r.target = Some(Format::Pdf);
    let out = run(&r).unwrap();
    let info = inspect(&out.output_path).unwrap();
    // 121 lines (one wrapped over several) need 3 pages, plus one after the form feed.
    assert_eq!(info.pages, Some(4), "{:?}", out.notes);
    let source = std::fs::read_to_string(&src).unwrap();
    assert_eq!(squash(&pdf_text(&out.output_path)), squash(&source));
    assert!(out.notes.iter().any(|n| n.contains("Courier")));

    // Non-Latin text embeds a font that has the glyphs.
    let mut r = req(
        &fixture("greek.txt"),
        OperationId::Convert,
        dir.path(),
        "greek",
    );
    r.target = Some(Format::Pdf);
    let out = run(&r).unwrap();
    assert!(pdf_text(&out.output_path).contains("Καλημέρα κόσμε"));
    assert!(out.notes.iter().any(|n| n.contains("DejaVu")));

    let empty = dir.path().join("empty.txt");
    std::fs::write(&empty, "  \n").unwrap();
    let mut r = req(&empty, OperationId::Convert, dir.path(), "empty");
    r.target = Some(Format::Pdf);
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidFile);
}

#[test]
fn word_properties_view_edit_and_strip() {
    let src = fixture("memo.docx");
    let report = docx::read_metadata(&src).unwrap();
    let value = |r: &radial_core::metadata::MetadataReport, k: &str| {
        r.fields
            .iter()
            .find(|f| f.key == k)
            .and_then(|f| f.value.clone())
    };
    assert_eq!(value(&report, "title").as_deref(), Some("Client memo"));
    assert_eq!(value(&report, "author").as_deref(), Some("Old author"));
    assert!(report
        .details
        .iter()
        .any(|d| d.label == "Last modified by" && d.value == "Jane Doe"));

    let dir = tempfile::tempdir().unwrap();
    let mut r = req(&src, OperationId::Metadata, dir.path(), "memo-edited");
    r.metadata.fields = BTreeMap::from([
        ("author".to_string(), "Account team".to_string()),
        ("keywords".to_string(), String::new()),
    ]);
    let out = run(&r).unwrap();
    assert_eq!(out.format, Format::Docx);
    let after = docx::read_metadata(&out.output_path).unwrap();
    assert_eq!(value(&after, "author").as_deref(), Some("Account team"));
    assert_eq!(value(&after, "keywords"), None);
    assert_eq!(value(&after, "title").as_deref(), Some("Client memo"));
    // Still a Word document with the same text; python-docx can open it too.
    assert_eq!(
        inspect(&out.output_path).unwrap().format,
        Some(Format::Docx)
    );
    assert_eq!(
        docx::to_text(&out.output_path).unwrap(),
        docx::to_text(&src).unwrap()
    );

    let mut r = req(&src, OperationId::Metadata, dir.path(), "memo-clean");
    r.metadata.strip_all = true;
    let out = run(&r).unwrap();
    let after = docx::read_metadata(&out.output_path).unwrap();
    assert!(after.fields.iter().all(|f| f.value.is_none()));
    assert!(!after.details.iter().any(|d| d.label == "Last modified by"));
    let raw = std::fs::read(&out.output_path).unwrap();
    let mut a = zip::ZipArchive::new(std::io::Cursor::new(raw)).unwrap();
    let mut core = String::new();
    std::io::Read::read_to_string(&mut a.by_name("docProps/core.xml").unwrap(), &mut core).unwrap();
    assert!(
        !core.contains("Jane") && !core.contains("Old author"),
        "{core}"
    );
}

/// The radial menu offers exactly these operations per file type, and the
/// backend refuses every operation it doesn't offer.
#[test]
fn operation_matrix_matches_real_support() {
    use OperationId::*;
    let dir = tempfile::tempdir().unwrap();
    let img = |name: &str, f: image::ImageFormat, alpha: bool| {
        let p = dir.path().join(name);
        let im = image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([10, 20, 30, if alpha { 0 } else { 255 }]),
        );
        let im = if f == image::ImageFormat::Jpeg || f == image::ImageFormat::Bmp {
            image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(im).to_rgb8())
        } else {
            image::DynamicImage::ImageRgba8(im)
        };
        im.save_with_format(&p, f).unwrap();
        p
    };
    let all = [
        Convert, Compress, Edit, Crop, Annotate, Redact, Background, Metadata,
    ];
    let images = [
        Convert, Compress, Edit, Crop, Annotate, Redact, Background, Metadata,
    ];
    let cases: Vec<(PathBuf, Vec<OperationId>)> = vec![
        (img("a.png", image::ImageFormat::Png, true), images.to_vec()),
        (
            img("a.jpg", image::ImageFormat::Jpeg, false),
            images.to_vec(),
        ),
        (
            img("a.webp", image::ImageFormat::WebP, true),
            images.to_vec(),
        ),
        (
            img("a.tiff", image::ImageFormat::Tiff, true),
            images.to_vec(),
        ),
        (
            img("a.bmp", image::ImageFormat::Bmp, false),
            vec![Convert, Edit, Crop, Annotate, Redact, Background, Metadata],
        ),
        (
            img("a.gif", image::ImageFormat::Gif, true),
            vec![Convert, Edit, Crop, Annotate, Redact, Background, Metadata],
        ),
        (
            fixture("report.pdf"),
            vec![Convert, Compress, Crop, Annotate, Redact, Metadata],
        ),
        (fixture("restricted.pdf"), vec![Metadata]),
        (fixture("memo.docx"), vec![Convert, Metadata]),
        (fixture("notes.txt"), vec![Convert]),
    ];
    let reg = Registry::new();
    for (path, expected) in cases {
        let info = inspect(&path).unwrap();
        let ops = ops::operations_for(&reg, &info);
        assert_eq!(ops.len(), all.len(), "{path:?}: every action is listed");
        let available: Vec<_> = ops.iter().filter(|o| o.available).map(|o| o.id).collect();
        assert_eq!(available, expected, "{path:?}");
        for o in ops.iter().filter(|o| !o.available) {
            assert!(o.reason.is_some(), "{path:?} {:?} needs a reason", o.id);
            let r = req(&path, o.id, dir.path(), "refused");
            let e = run(&r).expect_err("unavailable operation must be refused");
            assert_eq!(
                e.kind,
                ErrorKind::UnsupportedConversion,
                "{path:?} {:?}",
                o.id
            );
        }
    }
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        6,
        "nothing was written"
    );
}
