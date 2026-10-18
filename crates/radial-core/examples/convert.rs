//! Developer CLI over the conversion core, handy for manual QA:
//!
//!   cargo run -p radial-core --example convert -- <source> <png|jpeg|webp|bmp|tiff|gif> [dest-dir]
//!
//! Prints the detected file info, then converts (never overwriting) and
//! prints the outcome or the classified error.

use radial_core::engine::{self, ConvertRequest};
use radial_core::{detect, naming, Format, Registry};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [src, target, rest @ ..] = args.as_slice() else {
        eprintln!("usage: convert <source> <format> [dest-dir]");
        std::process::exit(2);
    };
    let source = std::fs::canonicalize(src).unwrap_or_else(|e| {
        eprintln!("cannot open {src}: {e}");
        std::process::exit(1)
    });
    let Some(target) = Format::parse(target) else {
        eprintln!("unknown format {target}");
        std::process::exit(2)
    };
    let dest = rest
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.parent().unwrap().to_path_buf());
    let dest = std::fs::canonicalize(dest).expect("destination folder");

    match detect::inspect(&source) {
        Ok(info) => println!(
            "detected: {:?} format={:?} {}x{} alpha={} depth={:?} problem={:?}",
            info.recognized,
            info.format,
            info.width.unwrap_or(0),
            info.height.unwrap_or(0),
            info.has_alpha,
            info.bit_depth,
            info.problem.as_ref().map(|p| (p.kind, &p.message))
        ),
        Err(e) => println!("inspect error: {:?} {}", e.kind, e.message),
    }
    let stem = naming::unique_path(&dest, &naming::default_stem(&source), target, Some(&source))
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let req = ConvertRequest {
        source,
        target,
        dest_dir: dest,
        stem,
        options: Default::default(),
    };
    match engine::convert(&Registry::new(), &req, &AtomicBool::new(false), |s| {
        eprintln!("  {s:?}")
    }) {
        Ok(o) => println!(
            "ok: {} ({}x{}, {} bytes)",
            o.output_path.display(),
            o.width,
            o.height,
            o.size_bytes
        ),
        Err(e) => println!("error: {:?} — {}", e.kind, e.message),
    }
}
