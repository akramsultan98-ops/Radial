//! Integration tests for operations (Compress, Edit, Crop, Annotate, Redact,
//! Background, Metadata) on real files in temporary directories.

use image::{DynamicImage, GenericImageView, Rgba};
use radial_core::detect::inspect;
use radial_core::draw::{Shape, ShapeKind};
use radial_core::metadata::{self, MetadataEdit};
use radial_core::ops::{self, OperationId, OperationRequest};
use radial_core::options::{ConvertOptions, CropRect, Edits};
use radial_core::{ErrorKind, Format, Registry};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn noisy(w: u32, h: u32, alpha: bool) -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::from_fn(w, h, |x, y| {
        let v = ((x * 7919 + y * 104_729) % 251) as u8;
        let a = if alpha && x < w / 4 { 0 } else { 255 };
        Rgba([v, v.wrapping_mul(3), (x + y) as u8, a])
    }))
}

fn save(dir: &Path, name: &str, img: &DynamicImage, fmt: image::ImageFormat) -> PathBuf {
    let p = dir.join(name);
    let img = if fmt == image::ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(img.to_rgb8())
    } else {
        img.clone()
    };
    img.save_with_format(&p, fmt).unwrap();
    p
}

fn save_jpeg_q(dir: &Path, name: &str, img: &DynamicImage, q: u8) -> PathBuf {
    let p = dir.join(name);
    let f = std::fs::File::create(&p).unwrap();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(f, q);
    DynamicImage::ImageRgb8(img.to_rgb8())
        .write_with_encoder(enc)
        .unwrap();
    p
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
    }
}

fn run(r: &OperationRequest) -> radial_core::Result<ops::OperationOutcome> {
    ops::run(&Registry::new(), r, &AtomicBool::new(false), |_| {})
}

fn decode(p: &Path) -> DynamicImage {
    image::ImageReader::open(p)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

// ---------------------------------------------------------------- registry

#[test]
fn operations_depend_on_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let reg = Registry::new();
    let png = save(
        dir.path(),
        "a.png",
        &noisy(32, 32, true),
        image::ImageFormat::Png,
    );
    let ops = ops::operations_for(&reg, &inspect(&png).unwrap());
    let ids: Vec<_> = ops.iter().map(|o| o.id).collect();
    assert_eq!(ids, OperationId::ALL.to_vec());
    assert!(ops.iter().all(|o| o.available));
    let convert = ops.iter().find(|o| o.id == OperationId::Convert).unwrap();
    assert_eq!(convert.targets.len(), 5);

    let bmp = save(
        dir.path(),
        "b.bmp",
        &noisy(32, 32, false),
        image::ImageFormat::Bmp,
    );
    let ops = ops::operations_for(&reg, &inspect(&bmp).unwrap());
    let compress = ops.iter().find(|o| o.id == OperationId::Compress).unwrap();
    assert!(!compress.available);
    assert!(compress
        .reason
        .as_deref()
        .unwrap()
        .contains("Convert to PNG"));
    let meta = ops.iter().find(|o| o.id == OperationId::Metadata).unwrap();
    assert!(meta.limitations.iter().any(|l| l.contains("not edited")));

    // Unreadable files get no operations at all.
    let junk = dir.path().join("junk.png");
    std::fs::write(&junk, b"not an image").unwrap();
    assert!(ops::operations_for(&reg, &inspect(&junk).unwrap()).is_empty());
}

#[test]
fn convert_operation_matches_conversion_engine() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "c.png",
        &noisy(40, 30, true),
        image::ImageFormat::Png,
    );
    let mut r = req(&src, OperationId::Convert, dir.path(), "c");
    assert_eq!(
        run(&r).unwrap_err().kind,
        ErrorKind::InvalidOptions,
        "target required"
    );
    r.target = Some(Format::Webp);
    let out = run(&r).unwrap();
    assert_eq!(
        inspect(&out.output_path).unwrap().format,
        Some(Format::Webp)
    );
    r.target = Some(Format::Png);
    r.stem = "c2".into();
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::UnsupportedConversion);
}

// ---------------------------------------------------------------- compress

#[test]
fn compress_jpeg_reports_real_savings() {
    let dir = tempfile::tempdir().unwrap();
    let src = save_jpeg_q(dir.path(), "big.jpg", &noisy(400, 300, false), 98);
    let mut r = req(&src, OperationId::Compress, dir.path(), "big-compressed");
    r.options.quality = Some(40);
    let out = run(&r).unwrap();
    assert!(
        out.size_bytes < out.source_size_bytes,
        "{} !< {}",
        out.size_bytes,
        out.source_size_bytes
    );
    assert_eq!(
        out.source_size_bytes,
        std::fs::metadata(&src).unwrap().len()
    );
    assert_eq!(decode(&out.output_path).dimensions(), (400, 300));
    assert_eq!(
        inspect(&out.output_path).unwrap().format,
        Some(Format::Jpeg)
    );
}

#[test]
fn compress_never_saves_a_larger_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = save_jpeg_q(dir.path(), "small.jpg", &noisy(200, 150, false), 20);
    let mut r = req(&src, OperationId::Compress, dir.path(), "small-compressed");
    r.options.quality = Some(100);
    let err = run(&r).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NoReduction);
    assert_eq!(
        names(dir.path()),
        vec!["small.jpg".to_string()],
        "nothing may be written"
    );
}

#[test]
fn compress_png_is_lossless() {
    let dir = tempfile::tempdir().unwrap();
    // A deliberately poorly compressed PNG.
    let src = dir.path().join("loose.png");
    {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(300, 200, |x, y| {
            Rgba([(x / 10) as u8 * 8, (y / 10) as u8 * 8, 90, 255])
        }));
        let f = std::fs::File::create(&src).unwrap();
        let enc = image::codecs::png::PngEncoder::new_with_quality(
            f,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        img.write_with_encoder(enc).unwrap();
    }
    let out = run(&req(
        &src,
        OperationId::Compress,
        dir.path(),
        "loose-compressed",
    ))
    .unwrap();
    assert!(out.size_bytes < out.source_size_bytes);
    assert_eq!(
        decode(&out.output_path).to_rgba8(),
        decode(&src).to_rgba8(),
        "pixels must be identical"
    );
}

#[test]
fn compress_can_limit_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let src = save_jpeg_q(dir.path(), "wide.jpg", &noisy(800, 400, false), 90);
    let mut r = req(&src, OperationId::Compress, dir.path(), "wide-small");
    r.max_dimension = Some(200);
    let out = run(&r).unwrap();
    assert_eq!((out.width, out.height), (Some(200), Some(100)));
}

#[test]
fn compress_is_refused_for_bmp() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "b.bmp",
        &noisy(32, 32, false),
        image::ImageFormat::Bmp,
    );
    assert_eq!(
        run(&req(&src, OperationId::Compress, dir.path(), "b2"))
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedConversion
    );
}

// ---------------------------------------------------------------- edit & crop

#[test]
fn edit_and_crop_keep_the_format() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "e.webp",
        &noisy(80, 40, true),
        image::ImageFormat::WebP,
    );
    let mut r = req(&src, OperationId::Edit, dir.path(), "e-edited");
    assert_eq!(
        run(&r).unwrap_err().kind,
        ErrorKind::InvalidOptions,
        "identity edit refused"
    );
    r.options.edits = Edits {
        rotate: 90,
        crop: None,
        brightness: 20,
        contrast: 0,
    };
    let out = run(&r).unwrap();
    assert_eq!(out.format, Format::Webp);
    assert_eq!(decode(&out.output_path).dimensions(), (40, 80));

    let mut c = req(&src, OperationId::Crop, dir.path(), "e-cropped");
    assert_eq!(
        run(&c).unwrap_err().kind,
        ErrorKind::InvalidOptions,
        "crop area required"
    );
    c.options.edits.crop = Some(CropRect {
        x: 0.5,
        y: 0.0,
        width: 0.5,
        height: 0.5,
    });
    // Unrelated edits sent with a crop request are ignored.
    c.options.edits.brightness = 90;
    let out = run(&c).unwrap();
    assert_eq!(decode(&out.output_path).dimensions(), (40, 20));
}

// ---------------------------------------------------------------- annotate

fn shape(kind: ShapeKind) -> Shape {
    Shape {
        kind,
        x1: 0.1,
        y1: 0.5,
        x2: 0.9,
        y2: 0.5,
        color: "#ff0000".into(),
        stroke: 0.05,
        filled: false,
        text: String::new(),
        size: 0.1,
    }
}

#[test]
fn annotations_are_part_of_the_exported_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let white = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        200,
        100,
        image::Rgb([255, 255, 255]),
    ));
    let src = save(dir.path(), "w.png", &white, image::ImageFormat::Png);
    let mut r = req(&src, OperationId::Annotate, dir.path(), "w-annotated");
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
    let mut text = shape(ShapeKind::Text);
    text.text = "Approved ✓".into();
    text.x1 = 0.05;
    text.y1 = 0.05;
    r.options.annotations = vec![shape(ShapeKind::Arrow), text];
    let out = run(&r).unwrap();
    let img = decode(&out.output_path).to_rgb8();
    assert_eq!(img.get_pixel(100, 50).0, [255, 0, 0], "arrow line present");
    let top = (0..40).flat_map(|y| (0..120).map(move |x| (x, y)));
    assert!(
        top.filter(|&(x, y)| img.get_pixel(x, y).0 != [255, 255, 255])
            .count()
            > 30,
        "text present"
    );
}

// ---------------------------------------------------------------- redact

#[test]
fn redaction_destroys_pixels_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("photo-gps.jpg");
    let before = metadata::read_image(&src, Format::Jpeg).unwrap();
    assert!(before.has_location);
    let mut r = req(&src, OperationId::Redact, dir.path(), "photo-redacted");
    r.options.redactions = vec![CropRect {
        x: 0.0,
        y: 0.0,
        width: 0.5,
        height: 1.0,
    }];
    // Annotations smuggled into a redaction request are ignored.
    r.options.annotations = vec![shape(ShapeKind::Line)];
    let out = run(&r).unwrap();
    let img = decode(&out.output_path).to_rgb8();
    for (x, y) in [(2, 2), (10, 40), (31, 24)] {
        let p = img.get_pixel(x, y).0;
        assert!(
            p.iter().all(|c| *c < 12),
            "pixel ({x},{y}) not black: {p:?}"
        );
    }
    let after = metadata::read_image(&out.output_path, Format::Jpeg).unwrap();
    assert!(!after.has_location);
    assert!(
        after.details.is_empty(),
        "no camera metadata may remain: {:?}",
        after.details
    );
    assert!(after.fields.iter().all(|f| f.value.is_none()));
}

// ---------------------------------------------------------------- background

#[test]
fn background_fills_transparency_and_pads() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "t.png",
        &noisy(40, 20, true),
        image::ImageFormat::Png,
    );
    let mut r = req(&src, OperationId::Background, dir.path(), "t-bg");
    r.options.background = Some("#00ff00".into());
    r.options.padding = 5;
    let out = run(&r).unwrap();
    let img = decode(&out.output_path);
    assert_eq!(img.dimensions(), (50, 30));
    assert!(!img.color().has_alpha() || img.to_rgba8().pixels().all(|p| p[3] == 255));
    let rgb = img.to_rgb8();
    assert_eq!(rgb.get_pixel(0, 0).0, [0, 255, 0], "padding");
    assert_eq!(
        rgb.get_pixel(7, 10).0,
        [0, 255, 0],
        "former transparent area"
    );

    let opaque = save(
        dir.path(),
        "o.png",
        &DynamicImage::ImageRgb8(noisy(20, 20, false).to_rgb8()),
        image::ImageFormat::Png,
    );
    let r = req(&opaque, OperationId::Background, dir.path(), "o-bg");
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
}

// ---------------------------------------------------------------- metadata

fn scan_data(path: &Path) -> Vec<u8> {
    // Everything from the first SOS marker: the compressed image itself.
    let d = std::fs::read(path).unwrap();
    let i = d.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    d[i..].to_vec()
}

#[test]
fn jpeg_metadata_is_read() {
    let r = metadata::read_image(&fixture("photo-gps.jpg"), Format::Jpeg).unwrap();
    assert!(r.can_edit && r.can_strip && r.has_location);
    let val = |k: &str| {
        r.fields
            .iter()
            .find(|f| f.key == k)
            .and_then(|f| f.value.clone())
    };
    assert_eq!(val("title").as_deref(), Some("Old title"));
    assert_eq!(val("author").as_deref(), Some("Old author"));
    assert!(r
        .details
        .iter()
        .any(|d| d.label == "Camera make" && d.value.contains("RADIAL Test Cam")));
}

#[test]
fn jpeg_metadata_edit_is_lossless_and_keeps_other_fields() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("photo-gps.jpg");
    let mut r = req(&src, OperationId::Metadata, dir.path(), "photo-meta");
    r.metadata.fields = BTreeMap::from([
        ("title".to_string(), "Q3 campaign — حملة 夏".to_string()),
        ("author".to_string(), "Account team".to_string()),
        ("copyright".to_string(), String::new()),
    ]);
    let out = run(&r).unwrap();
    assert_eq!(
        scan_data(&out.output_path),
        scan_data(&src),
        "image data must be byte-identical"
    );
    let m = metadata::read_image(&out.output_path, Format::Jpeg).unwrap();
    let val = |k: &str| {
        m.fields
            .iter()
            .find(|f| f.key == k)
            .and_then(|f| f.value.clone())
    };
    assert_eq!(val("title").as_deref(), Some("Q3 campaign — حملة 夏"));
    assert_eq!(val("author").as_deref(), Some("Account team"));
    assert!(m.has_location, "untouched fields stay");
    assert!(m.details.iter().any(|d| d.label == "Taken"));
}

#[test]
fn jpeg_strip_removes_everything_but_orientation() {
    let dir = tempfile::tempdir().unwrap();
    for (name, oriented) in [("photo-gps.jpg", false), ("exif-orientation-6.jpg", true)] {
        let src = fixture(name);
        let mut r = req(
            &src,
            OperationId::Metadata,
            dir.path(),
            &format!("{name}-stripped"),
        );
        r.metadata.strip_all = true;
        let out = run(&r).unwrap();
        assert_eq!(scan_data(&out.output_path), scan_data(&src));
        let m = metadata::read_image(&out.output_path, Format::Jpeg).unwrap();
        assert!(!m.has_location);
        assert!(m.fields.iter().all(|f| f.value.is_none()));
        let info = inspect(&out.output_path).unwrap();
        let loaded = radial_core::engine::load(&out.output_path, Format::Jpeg)
            .unwrap()
            .image;
        if oriented {
            assert_eq!(
                loaded.dimensions(),
                (20, 40),
                "orientation must survive stripping"
            );
        }
        assert_eq!(info.format, Some(Format::Jpeg));
    }
}

#[test]
fn png_metadata_edit_and_strip() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "m.png",
        &noisy(30, 30, true),
        image::ImageFormat::Png,
    );
    let mut r = req(&src, OperationId::Metadata, dir.path(), "m-meta");
    r.metadata.fields = BTreeMap::from([
        ("title".into(), "Brochure 写真".into()),
        ("keywords".into(), "client, draft".into()),
    ]);
    let out = run(&r).unwrap();
    assert_eq!(decode(&out.output_path).to_rgba8(), decode(&src).to_rgba8());
    let m = metadata::read_image(&out.output_path, Format::Png).unwrap();
    let val = |k: &str| {
        m.fields
            .iter()
            .find(|f| f.key == k)
            .and_then(|f| f.value.clone())
    };
    assert_eq!(val("title").as_deref(), Some("Brochure 写真"));
    assert_eq!(val("keywords").as_deref(), Some("client, draft"));

    let mut s = req(
        &out.output_path,
        OperationId::Metadata,
        dir.path(),
        "m-stripped",
    );
    s.metadata.strip_all = true;
    let stripped = run(&s).unwrap();
    let m = metadata::read_image(&stripped.output_path, Format::Png).unwrap();
    assert!(m.fields.iter().all(|f| f.value.is_none()));
}

#[test]
fn metadata_rejects_unsupported_formats_and_empty_edits() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "w.webp",
        &noisy(16, 16, false),
        image::ImageFormat::WebP,
    );
    let mut r = req(&src, OperationId::Metadata, dir.path(), "w2");
    r.metadata.strip_all = true;
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::UnsupportedFormat);

    let png = save(
        dir.path(),
        "p.png",
        &noisy(16, 16, false),
        image::ImageFormat::Png,
    );
    let r = req(&png, OperationId::Metadata, dir.path(), "p2");
    assert_eq!(run(&r).unwrap_err().kind, ErrorKind::InvalidOptions);
}

#[test]
fn operations_never_touch_the_original_and_respect_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(
        dir.path(),
        "keep.png",
        &noisy(50, 50, true),
        image::ImageFormat::Png,
    );
    let before = std::fs::read(&src).unwrap();
    let mut r = req(&src, OperationId::Redact, dir.path(), "keep");
    r.options.redactions = vec![CropRect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    }];
    assert_eq!(
        run(&r).unwrap_err().kind,
        ErrorKind::DestinationExists,
        "same name as source"
    );
    r.stem = "keep-redacted".into();
    let cancel = AtomicBool::new(true);
    let err = ops::run(&Registry::new(), &r, &cancel, |_| {}).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Cancelled);
    assert_eq!(names(dir.path()), vec!["keep.png".to_string()]);
    assert_eq!(std::fs::read(&src).unwrap(), before);
}

#[test]
fn default_stems_never_match_the_source() {
    let p = Path::new("/x/report.png");
    assert_eq!(ops::default_stem(OperationId::Redact, p), "report-redacted");
    assert_eq!(ops::default_stem(OperationId::Convert, p), "report");
}
