//! `radial --self-test <report-file>`: a windowless check that an installed
//! build can load its bundled PDFium and run real operations. Used by CI on
//! the installed app; prints nothing (release builds have no console) but
//! writes a report and exits with 0 on success.

use radial_core::ops::{self, OperationId, OperationRequest};
use radial_core::{detect, Format, Registry};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn request(
    source: &Path,
    dir: &Path,
    stem: &str,
    op: OperationId,
    target: Format,
) -> OperationRequest {
    OperationRequest {
        source: source.to_path_buf(),
        operation: op,
        target: Some(target),
        dest_dir: dir.to_path_buf(),
        stem: stem.into(),
        options: Default::default(),
        metadata: Default::default(),
        max_dimension: None,
        pdf: Default::default(),
    }
}

fn checks(log: &mut Vec<String>) -> Result<(), String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
        .ok_or("no executable directory")?;
    radial_core::pdf::configure([exe_dir.join("pdfium")]);
    if let Some(why) = radial_core::pdf::missing_reason() {
        return Err(format!("PDF engine: {why}"));
    }
    log.push("PDF engine loaded".into());

    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let reg = Registry::new();
    let cancel = AtomicBool::new(false);
    let png = dir.path().join("sample.png");
    radial_core::image::RgbaImage::from_pixel(64, 48, radial_core::image::Rgba([200, 40, 40, 255]))
        .save(&png)
        .map_err(|e| e.to_string())?;

    let pdf = ops::run(
        &reg,
        &request(
            &png,
            dir.path(),
            "sample",
            OperationId::Convert,
            Format::Pdf,
        ),
        &cancel,
        |_| {},
    )
    .map_err(|e| format!("PNG → PDF: {}", e.message))?;
    let info = detect::inspect(&pdf.output_path).map_err(|e| e.message)?;
    if info.pages != Some(1) {
        return Err("PDF not recognised after writing".into());
    }
    log.push("PNG → PDF ok".into());

    let page = ops::run(
        &reg,
        &request(
            &pdf.output_path,
            dir.path(),
            "page",
            OperationId::Convert,
            Format::Png,
        ),
        &cancel,
        |_| {},
    )
    .map_err(|e| format!("PDF → PNG: {}", e.message))?;
    log.push(format!("PDF → PNG ok ({:?}×{:?})", page.width, page.height));

    let txt = dir.path().join("note.txt");
    std::fs::write(&txt, "Self-test ✓ Καλημέρα\n").map_err(|e| e.to_string())?;
    ops::run(
        &reg,
        &request(&txt, dir.path(), "note", OperationId::Convert, Format::Pdf),
        &cancel,
        |_| {},
    )
    .map_err(|e| format!("TXT → PDF: {}", e.message))?;
    log.push("TXT → PDF ok".into());
    Ok(())
}

/// Run the checks, write the report, and return the process exit code.
pub fn run(report: Option<PathBuf>) -> i32 {
    let mut log = vec![format!("RADIAL {} self-test", env!("CARGO_PKG_VERSION"))];
    let code = match checks(&mut log) {
        Ok(()) => {
            log.push("PASS".into());
            0
        }
        Err(e) => {
            log.push(format!("FAIL: {e}"));
            1
        }
    };
    let text = log.join("\n") + "\n";
    match report {
        Some(p) => {
            let _ = std::fs::write(p, text);
        }
        None => print!("{text}"),
    }
    code
}
