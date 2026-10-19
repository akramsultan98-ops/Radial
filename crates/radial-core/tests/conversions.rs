//! Integration tests: real files, real encoders, temporary directories.

use image::{DynamicImage, GenericImageView, ImageDecoder, ImageEncoder, Rgba};
use radial_core::detect::{self, inspect};
use radial_core::engine::{self, ConvertRequest, Stage};
use radial_core::format::Recognized;
use radial_core::options::{ConvertOptions, CropRect, Edits, Resize};
use radial_core::{ErrorKind, Format, Registry};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// 64×48 RGBA test card: opaque gradient with a fully transparent left quarter.
fn sample(transparent: bool) -> DynamicImage {
    DynamicImage::ImageRgba8(image::RgbaImage::from_fn(64, 48, |x, y| {
        let a = if transparent && x < 16 { 0 } else { 255 };
        Rgba([(x * 4) as u8, (y * 5) as u8, 200, a])
    }))
}

/// Write `img` as `format` using a codec independent of RADIAL's engine.
fn write_sample(dir: &Path, name: &str, format: Format, img: &DynamicImage) -> PathBuf {
    let path = dir.join(name);
    let img = match format {
        Format::Jpeg | Format::Bmp => DynamicImage::ImageRgb8(img.to_rgb8()),
        _ => img.clone(),
    };
    let fmt = match format {
        Format::Png => image::ImageFormat::Png,
        Format::Jpeg => image::ImageFormat::Jpeg,
        Format::Webp => image::ImageFormat::WebP,
        Format::Bmp => image::ImageFormat::Bmp,
        Format::Tiff => image::ImageFormat::Tiff,
        Format::Gif => image::ImageFormat::Gif,
        Format::Pdf | Format::Txt => unreachable!("not an image"),
    };
    img.save_with_format(&path, fmt).unwrap();
    path
}

fn request(source: &Path, target: Format, dir: &Path, stem: &str) -> ConvertRequest {
    ConvertRequest {
        source: source.to_path_buf(),
        target,
        dest_dir: dir.to_path_buf(),
        stem: stem.into(),
        options: ConvertOptions::default(),
    }
}

fn run(req: &ConvertRequest) -> radial_core::Result<radial_core::engine::ConvertOutcome> {
    engine::convert(&Registry::new(), req, &AtomicBool::new(false), |_| {})
}

fn decode(path: &Path) -> DynamicImage {
    image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}

fn dir_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn every_registered_conversion_produces_a_verified_file() {
    let registry = Registry::new();
    let dir = tempfile::tempdir().unwrap();
    let mut checked = 0;
    // Image → image only; PDF conversions are covered in tests/pdf.rs.
    for from in registry.sources().into_iter().filter(|f| f.is_image()) {
        let src = write_sample(
            dir.path(),
            &format!("src.{}", from.extension()),
            from,
            &sample(false),
        );
        assert_eq!(inspect(&src).unwrap().format, Some(from));
        for cap in registry
            .targets_for(from)
            .into_iter()
            .filter(|c| c.to.is_image())
        {
            let stem = format!("{}-to-{}", from.id(), cap.to.id());
            let out = run(&request(&src, cap.to, dir.path(), &stem))
                .unwrap_or_else(|e| panic!("{stem}: {}", e.message));
            assert!(out.output_path.exists(), "{stem}: missing output");
            let info = inspect(&out.output_path).unwrap();
            assert_eq!(info.format, Some(cap.to), "{stem}: wrong content type");
            assert_eq!(
                (info.width, info.height),
                (Some(64), Some(48)),
                "{stem}: wrong size"
            );
            assert_eq!(
                out.size_bytes,
                std::fs::metadata(&out.output_path).unwrap().len()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 30);
    // No temporary files left behind.
    assert!(dir_names(dir.path()).iter().all(|n| !n.ends_with(".part")));
}

#[test]
fn source_file_is_never_modified() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "a.png", Format::Png, &sample(true));
    let before = std::fs::read(&src).unwrap();
    let mtime = std::fs::metadata(&src).unwrap().modified().unwrap();
    run(&request(&src, Format::Jpeg, dir.path(), "a")).unwrap();
    assert_eq!(std::fs::read(&src).unwrap(), before);
    assert_eq!(std::fs::metadata(&src).unwrap().modified().unwrap(), mtime);
}

#[cfg(unix)]
#[test]
fn output_gets_normal_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "p.png", Format::Png, &sample(false));
    let out = run(&request(&src, Format::Jpeg, dir.path(), "p")).unwrap();
    // Compare with what an ordinary File::create gets under the same umask.
    let reference = dir.path().join("reference");
    std::fs::File::create(&reference).unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&out.output_path), mode(&reference));
}

#[cfg(unix)]
#[test]
fn read_only_source_converts() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "ro.png", Format::Png, &sample(false));
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o444)).unwrap();
    run(&request(&src, Format::Webp, dir.path(), "ro")).unwrap();
}

#[test]
fn transparency_is_preserved_when_target_supports_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "t.png", Format::Png, &sample(true));
    for target in [Format::Webp, Format::Tiff, Format::Gif] {
        let mut req = request(&src, target, dir.path(), &format!("t-{}", target.id()));
        if target == Format::Webp {
            req.options.lossless = true;
        }
        let out = run(&req).unwrap();
        let img = decode(&out.output_path);
        assert!(img.color().has_alpha(), "{target:?} lost alpha channel");
        assert_eq!(
            img.get_pixel(2, 2)[3],
            0,
            "{target:?} transparent pixel became opaque"
        );
        assert_eq!(
            img.get_pixel(40, 2)[3],
            255,
            "{target:?} opaque pixel became transparent"
        );
    }
    // Lossy WebP must keep alpha too.
    let out = run(&request(&src, Format::Webp, dir.path(), "t-lossy")).unwrap();
    assert_eq!(decode(&out.output_path).get_pixel(2, 2)[3], 0);
}

#[test]
fn transparency_is_flattened_onto_background_for_jpeg_and_bmp() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "t.png", Format::Png, &sample(true));
    for target in [Format::Jpeg, Format::Bmp] {
        let mut req = request(&src, target, dir.path(), &format!("bg-{}", target.id()));
        req.options.background = Some("#ff0000".into());
        req.options.quality = (target == Format::Jpeg).then_some(100);
        let out = run(&req).unwrap();
        let img = decode(&out.output_path);
        assert!(!img.color().has_alpha());
        let p = img.get_pixel(4, 4);
        assert!(
            p[0] > 240 && p[1] < 15 && p[2] < 15,
            "{target:?}: expected red background, got {p:?}"
        );
    }
}

#[test]
fn jpeg_quality_changes_output_size() {
    let dir = tempfile::tempdir().unwrap();
    let noisy = DynamicImage::ImageRgb8(image::RgbImage::from_fn(256, 256, |x, y| {
        let v = ((x * 7919 + y * 104_729) % 251) as u8;
        image::Rgb([v, v.wrapping_mul(3), v.wrapping_add(x as u8)])
    }));
    let src = write_sample(dir.path(), "n.png", Format::Png, &noisy);
    let size = |q: u8, target: Format, stem: &str| {
        let mut req = request(&src, target, dir.path(), stem);
        req.options.quality = Some(q);
        run(&req).unwrap().size_bytes
    };
    assert!(size(10, Format::Jpeg, "q10") < size(95, Format::Jpeg, "q95"));
    assert!(size(10, Format::Webp, "w10") < size(95, Format::Webp, "w95"));
}

#[test]
fn resize_preserves_aspect_ratio_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "r.png", Format::Png, &sample(false));
    let mut req = request(&src, Format::Jpeg, dir.path(), "r");
    req.options.resize = Some(Resize {
        width: Some(32),
        height: None,
        keep_aspect: true,
    });
    let out = run(&req).unwrap();
    assert_eq!((out.width, out.height), (32, 24));
    assert_eq!(decode(&out.output_path).dimensions(), (32, 24));
}

#[test]
fn edits_affect_exported_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "e.png", Format::Png, &sample(false));
    let mut req = request(&src, Format::Png, dir.path(), "e");
    req.target = Format::Tiff;
    req.options.edits = Edits {
        rotate: 90,
        crop: Some(CropRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 0.5,
        }),
        brightness: 50,
        contrast: 0,
    };
    let out = run(&req).unwrap();
    // 64×48 rotated → 48×64, top half → 48×32.
    assert_eq!(decode(&out.output_path).dimensions(), (48, 32));
    let plain = run(&request(&src, Format::Tiff, dir.path(), "plain")).unwrap();
    let bright = decode(&out.output_path).to_rgb8();
    let base = decode(&plain.output_path).to_rgb8();
    let mean = |b: &image::RgbImage| {
        b.pixels().map(|p| u64::from(p[1])).sum::<u64>() / u64::from(b.width() * b.height())
    };
    assert!(mean(&bright) > mean(&base));
}

#[test]
fn tiff_output_is_compressed_and_tags_alpha() {
    use tiff::decoder::Decoder;
    use tiff::tags::Tag;
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "a.png", Format::Png, &sample(true));
    let out = run(&request(&src, Format::Tiff, dir.path(), "a")).unwrap();
    let mut dec = Decoder::new(std::fs::File::open(&out.output_path).unwrap()).unwrap();
    assert_eq!(
        dec.get_tag_u32(Tag::Compression).unwrap(),
        5,
        "expected LZW"
    );
    assert_eq!(
        dec.get_tag_u32(Tag::ExtraSamples).unwrap(),
        2,
        "expected unassociated alpha"
    );
    assert_eq!(dec.get_tag_u32(Tag::SamplesPerPixel).unwrap(), 4);
    // Opaque RGB output must not claim an extra sample.
    let src = write_sample(dir.path(), "b.jpg", Format::Jpeg, &sample(false));
    let out = run(&request(&src, Format::Tiff, dir.path(), "b")).unwrap();
    let mut dec = Decoder::new(std::fs::File::open(&out.output_path).unwrap()).unwrap();
    assert!(dec.find_tag(Tag::ExtraSamples).unwrap().is_none());
    assert_eq!(dec.get_tag_u32(Tag::SamplesPerPixel).unwrap(), 3);
}

#[test]
fn exif_orientation_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(&request(
        &fixture("exif-orientation-6.jpg"),
        Format::Png,
        dir.path(),
        "o",
    ))
    .unwrap();
    let img = decode(&out.output_path).to_rgb8();
    assert_eq!(
        img.dimensions(),
        (20, 40),
        "orientation 6 should rotate 40×20 to 20×40"
    );
    let top = img.get_pixel(10, 2);
    let bottom = img.get_pixel(10, 37);
    assert!(
        top[2] > 200 && top[0] < 60,
        "blue band should be at the top, got {top:?}"
    );
    assert!(
        bottom[0] > 200 && bottom[2] < 60,
        "red should be at the bottom, got {bottom:?}"
    );
}

#[test]
fn color_profile_is_preserved_for_png_and_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("icc.png");
    let icc: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
    {
        let f = std::fs::File::create(&src).unwrap();
        let mut enc = image::codecs::png::PngEncoder::new(f);
        enc.set_icc_profile(icc.clone()).unwrap();
        sample(false).write_with_encoder(enc).unwrap();
    }
    for target in [Format::Jpeg, Format::Png] {
        let mut req = request(&src, target, dir.path(), &format!("icc-{}", target.id()));
        if target == Format::Png {
            req.source = run(&request(&src, Format::Jpeg, dir.path(), "icc-hop"))
                .unwrap()
                .output_path;
        }
        let out = run(&req).unwrap();
        let mut dec = image::ImageReader::open(&out.output_path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert_eq!(
            dec.icc_profile().unwrap().as_deref(),
            Some(icc.as_slice()),
            "{target:?} dropped ICC profile"
        );
    }
}

#[test]
fn sixteen_bit_png_converts() {
    let dir = tempfile::tempdir().unwrap();
    let img = DynamicImage::ImageRgba16(image::ImageBuffer::from_fn(20, 10, |x, _| {
        Rgba([x as u16 * 3000, 1000, 65535, 65535])
    }));
    let src = write_sample(dir.path(), "deep.png", Format::Png, &img);
    assert_eq!(inspect(&src).unwrap().bit_depth, Some(16));
    for target in [
        Format::Jpeg,
        Format::Webp,
        Format::Tiff,
        Format::Bmp,
        Format::Gif,
    ] {
        let out = run(&request(
            &src,
            target,
            dir.path(),
            &format!("deep-{}", target.id()),
        ))
        .unwrap();
        assert_eq!(decode(&out.output_path).dimensions(), (20, 10));
    }
    let out = run(&request(&src, Format::Tiff, dir.path(), "deep-keep")).unwrap();
    assert_eq!(
        decode(&out.output_path).color(),
        image::ColorType::Rgba16,
        "TIFF should keep 16-bit depth"
    );
}

#[test]
fn existing_destination_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "c.png", Format::Png, &sample(false));
    let existing = dir.path().join("c.jpg");
    std::fs::write(&existing, b"precious").unwrap();
    let err = run(&request(&src, Format::Jpeg, dir.path(), "c")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::DestinationExists);
    assert_eq!(std::fs::read(&existing).unwrap(), b"precious");
    // The suggested unique name works.
    let p = radial_core::naming::unique_path(dir.path(), "c", Format::Jpeg, Some(&src));
    assert_eq!(p.file_name().unwrap(), "c (2).jpg");
    run(&request(&src, Format::Jpeg, dir.path(), "c (2)")).unwrap();
}

#[test]
fn cancellation_before_and_during_conversion_leaves_no_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "x.png", Format::Png, &sample(false));
    let reg = Registry::new();

    let cancel = AtomicBool::new(true);
    let err = engine::convert(
        &reg,
        &request(&src, Format::Jpeg, dir.path(), "x"),
        &cancel,
        |_| {},
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Cancelled);

    for stop_at in [
        Stage::Decoding,
        Stage::Processing,
        Stage::Encoding,
        Stage::Verifying,
    ] {
        let cancel = AtomicBool::new(false);
        let mut stages = Vec::new();
        let err = engine::convert(
            &reg,
            &request(&src, Format::Jpeg, dir.path(), "x"),
            &cancel,
            |s| {
                stages.push(s);
                if s == stop_at {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Cancelled, "cancel at {stop_at:?}");
        assert_eq!(*stages.last().unwrap(), stop_at);
    }
    assert_eq!(
        dir_names(dir.path()),
        vec!["x.png".to_string()],
        "cancelled jobs left files behind"
    );
}

#[test]
fn stages_are_reported_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "s.png", Format::Png, &sample(false));
    let mut stages = Vec::new();
    engine::convert(
        &Registry::new(),
        &request(&src, Format::Webp, dir.path(), "s"),
        &AtomicBool::new(false),
        |s| stages.push(s),
    )
    .unwrap();
    assert_eq!(
        stages,
        vec![
            Stage::Decoding,
            Stage::Processing,
            Stage::Encoding,
            Stage::Verifying
        ]
    );
}

#[test]
fn unicode_paths_work() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("фото 写真 🌅");
    std::fs::create_dir(&dir).unwrap();
    let src = write_sample(&dir, "картинка — 画像.png", Format::Png, &sample(true));
    let info = inspect(&src).unwrap();
    assert_eq!(info.name, "картинка — 画像.png");
    let out = run(&request(&src, Format::Webp, &dir, "結果 ✓")).unwrap();
    assert_eq!(
        out.output_path.file_name().unwrap().to_string_lossy(),
        "結果 ✓.webp"
    );
    assert!(out.output_path.exists());
}

#[test]
fn detection_uses_content_not_extension() {
    let dir = tempfile::tempdir().unwrap();
    let png_named_jpg = write_sample(dir.path(), "real.png", Format::Png, &sample(false));
    let renamed = dir.path().join("liar.jpg");
    std::fs::rename(&png_named_jpg, &renamed).unwrap();
    let info = inspect(&renamed).unwrap();
    assert_eq!(info.format, Some(Format::Png));
    assert!(info.extension_mismatch);
    // And it converts as PNG.
    let out = run(&request(&renamed, Format::Webp, dir.path(), "liar")).unwrap();
    assert_eq!(out.source_format, Format::Png);
}

#[test]
fn invalid_inputs_are_classified() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let p = dir.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    };
    let problem = |p: &Path| inspect(p).unwrap().problem.map(|e| e.kind);

    assert_eq!(
        problem(&write("empty.png", b"")),
        Some(ErrorKind::EmptyFile)
    );
    assert_eq!(
        problem(&write("text.png", b"just some text, not an image")),
        Some(ErrorKind::InvalidFile)
    );
    assert_eq!(
        problem(&write("doc.pdf", b"%PDF-1.7\n...")),
        Some(ErrorKind::CorruptFile)
    );

    let good = std::fs::read(write_sample(
        dir.path(),
        "good.png",
        Format::Png,
        &sample(false),
    ))
    .unwrap();
    // Truncated mid-header: signature is valid, contents are not.
    let truncated = write("truncated.png", &good[..20]);
    assert_eq!(problem(&truncated), Some(ErrorKind::CorruptFile));
    // Header OK but pixel data cut off: detected as PNG, conversion fails cleanly.
    let cut = write("cut.png", &good[..good.len() / 2]);
    let info = inspect(&cut).unwrap();
    assert_eq!(info.format, Some(Format::Png));
    let err = run(&request(&cut, Format::Jpeg, dir.path(), "cut")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::CorruptFile);
    assert!(!dir.path().join("cut.jpg").exists());
    assert!(dir_names(dir.path()).iter().all(|n| !n.ends_with(".part")));

    // Directory and missing file.
    assert_eq!(
        inspect(dir.path()).unwrap_err().kind,
        ErrorKind::InvalidFile
    );
    assert_eq!(
        inspect(&dir.path().join("missing.png")).unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[test]
fn oversized_dimensions_are_rejected_before_decoding() {
    // A valid PNG header claiming 40 000 × 40 000 pixels (a decompression bomb).
    fn crc32(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&40_000u32.to_be_bytes());
    ihdr.extend_from_slice(&40_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("bomb.png");
    std::fs::write(&p, &png).unwrap();
    assert_eq!(
        inspect(&p).unwrap().problem.map(|e| e.kind),
        Some(ErrorKind::TooLarge)
    );
}

#[test]
fn unsupported_and_invalid_requests_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_sample(dir.path(), "u.png", Format::Png, &sample(false));
    let err = run(&request(&src, Format::Png, dir.path(), "u2")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::UnsupportedConversion);

    let err = run(&request(&src, Format::Jpeg, dir.path(), "bad/name")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidOptions);

    let err = run(&request(&src, Format::Jpeg, &dir.path().join("nope"), "u")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::DestinationNotWritable);

    let mut req = request(&src, Format::Png, dir.path(), "u");
    req.target = Format::Bmp;
    req.options.quality = Some(50);
    assert_eq!(run(&req).unwrap_err().kind, ErrorKind::InvalidOptions);
}

#[test]
fn preview_is_downscaled_and_reflects_edits() {
    let dir = tempfile::tempdir().unwrap();
    let big = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        1200,
        600,
        image::Rgb([10, 200, 30]),
    ));
    let src = write_sample(dir.path(), "big.png", Format::Png, &big);
    let base = engine::preview_base(&src, 0, 300).unwrap();
    assert_eq!(base.dimensions(), (300, 150));
    let bytes = engine::render_preview(
        &base,
        &Edits {
            rotate: 90,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        detect::sniff(&bytes),
        Some(Recognized::Convertible(Format::Jpeg))
    );
    assert_eq!(
        image::load_from_memory(&bytes).unwrap().dimensions(),
        (150, 300)
    );

    let t = write_sample(dir.path(), "t.png", Format::Png, &sample(true));
    let bytes = engine::render_preview(
        &engine::preview_base(&t, 0, 512).unwrap(),
        &Edits::default(),
    )
    .unwrap();
    assert_eq!(
        detect::sniff(&bytes),
        Some(Recognized::Convertible(Format::Png)),
        "transparent previews stay PNG"
    );
}
