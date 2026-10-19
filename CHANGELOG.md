# Changelog

All notable changes to RADIAL are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.2.0] — 2026-10-19

Interactive file toolkit: the radial menu offers actions for images, PDFs, Word documents and
text files.

### Added
- Operations: the radial menu now offers actions for the selected file — Convert, Compress, Edit,
  Crop, Annotate, Redact, Background and Metadata — chosen by click, drag or keyboard, with an
  accessible list alternative. Convert opens a second ring of output formats.
- Typed operation registry (`ops.rs`): availability, reasons, options and limitations per file;
  the UI renders only what it returns.
- Compress (JPEG/WebP quality, lossless PNG optimisation, size limit). Results that are not smaller
  are discarded and reported, never saved.
- Redact for images: marked areas are replaced with black pixels; all metadata is removed.
- Annotate for images: arrows, lines, boxes, ellipses and text drawn into the exported pixels.
- Background: fill transparency with a colour and/or add a border.
- Metadata: read EXIF/PNG text, edit title/author/subject/keywords/copyright losslessly
  (JPEG/PNG), or remove all metadata while keeping orientation.
- History records the action; annotation text, redaction areas and metadata values are never stored.
- PDF engine: PDFium (BSD-3, pinned `chromium/7881`, bundled with its licences) plus lopdf.
  - Detection with page count, page sizes, password/restriction/signature status.
  - Convert a page to PNG/JPEG (72–600 DPI) or the whole document to text (no OCR).
  - Compress (re-encode JPEG photos, compress streams, drop unused objects); refused if not
    smaller or if any page's text changed.
  - Crop one page or all pages; annotate with vector shapes and text (rotation-aware).
  - Genuine redaction: affected pages are rebuilt as images with the areas blacked out; old
    content, form fields, thumbnails, tags and metadata are removed and the result is verified.
    Text search redacts a word everywhere; text that also lives in bookmarks or form values is
    reported and the redaction refused.
  - Document properties: view, edit title/author/subject/keywords, remove Info and XMP.
- Any image → one-page A4 PDF (JPEG embedded unchanged).
- Word (.docx): extract text; view, edit and remove document properties.
- Plain text (.txt) → PDF (Courier, or embedded DejaVu Sans for non-Latin text).
- Page navigation in every editor and result view; PDF file details; text/document result screen.
- Third-party notices (all Rust crates, DejaVu, PDFium) shipped with the installer.

### Changed
- Disabled actions stay visible in the ring with the reason, and the backend refuses them too.

### Fixed
- Re-opening the same file from history now reloads its actions.

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
