# Changelog

All notable changes to RADIAL are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] — 2026-10-18

First working release: local image conversion around a radial format selector.

### Added
- Radial format selector: choose an output by clicking a segment, dragging the file onto it, or
  with the keyboard (arrows, Enter/Space, 1–9); an equivalent list in the side panel.
- Content-based file detection that distinguishes recognised, readable and convertible files and
  explains rejections (PDF, HEIC, MP4, executables and more are identified by signature).
- Capability registry as the single source of truth for conversions, options and limitations.
- Image engine: PNG, JPEG, WebP, BMP, TIFF and GIF in all 30 combinations; JPEG/WebP quality,
  lossless WebP, background colour for transparency, resize with aspect lock.
- Non-destructive editing applied to the export: rotate, crop, brightness, contrast.
- Before/after preview with fit, zoom and actual size, rendered by the native pipeline.
- EXIF orientation applied; ICC profiles preserved in PNG and JPEG; 16-bit kept in PNG/TIFF.
- TIFF output with LZW compression and a correct alpha (ExtraSamples) tag.
- Safe output: never overwrites, atomic write to a hidden temp file, verification by re-decoding,
  normal file permissions; staged progress and cancellation that never leaves partial files.
- Local history (open, show in folder, retry, clear) and settings (default folder, default
  quality, background, motion, history on/off).
- Path allowlist in the desktop shell; the webview has no file-system permissions.
- Windows installer configuration (MSI + NSIS) and CI that builds them on Windows.
- Tests: Rust unit and integration tests on real files, Vitest unit/component tests, Playwright UI
  tests; native QA under Xvfb with screenshots.
