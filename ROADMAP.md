# Roadmap

Priorities are ordered. A format is only offered once its conversion is implemented and tested.

## ✅ Phase 1 — Image conversion (v0.1)

- Radial format selector with drag, click and keyboard selection, plus an accessible list
- Content-based detection; capability registry as the single source of truth
- PNG / JPEG / WebP / BMP / TIFF / GIF, all 30 pairs, with quality, lossless WebP, background
  colour, resize
- Non-destructive edits (rotate, crop, brightness, contrast) that apply to the export
- Before/after preview with fit, zoom and 100 %
- Atomic, verified, no-overwrite output; cancellation; staged progress
- Local history with open / reveal / retry; settings
- Windows installers (unsigned) from CI

## ✅ v0.2 — Interactive file toolkit

- Operation ring (Convert, Compress, Edit, Crop, Annotate, Redact, Background, Metadata) driven
  by a typed operation registry; unavailable actions show why
- Images: compression that never saves a larger file, pixel redaction with metadata removal,
  annotations, background/border, lossless EXIF/PNG metadata editing
- PDF (PDFium, bundled): page export, text extraction, compression, crop, annotations, genuine
  redaction (page rebuilt as an image, verified), document properties
- Documents: DOCX → text, DOCX properties, TXT → PDF

## Phase 1.1 — Polish and distribution

- Code-signing for Windows installers (needs a certificate — see `docs/PACKAGING.md`)
- File association / "Open with RADIAL" registration in the installer (launch-argument
  handling already works)
- Single-instance behaviour: a second launch hands its file to the running window
- Metadata option on Convert: keep or strip EXIF (currently always stripped, orientation applied)
- PDF: keep searchable text on redacted pages (selective glyph removal), page reorder/delete,
  merge/split, OCR for scanned PDFs
- Text annotations with complex-script shaping (Arabic, Hebrew)
- ICC profile embedding for WebP and TIFF output; optional conversion to sRGB
- Remember window size/position

## Phase 2 — Batch

- Convert every file in the tray to one format, with a per-file result list
- Parallelism already bounded by the job queue (2 concurrent conversions)

## Phase 3 — Audio and video (FFmpeg)

- Engine probe: locate a user-installed `ffmpeg` (or a bundled build — evaluate licensing:
  LGPL builds only, no GPL-only encoders) and expose availability in the registry
- MP3, WAV, AAC/M4A, FLAC, Ogg; MP4 (H.264/AAC), WebM (VP9/Opus); audio extraction from video
- Options: bitrate, sample rate, channels, resolution, quality (CRF)
- Real progress from FFmpeg's `-progress` output (time-based, honest percentages)
- Cancellation by terminating the child process; temp-file contract unchanged

## Phase 4 — Documents (LibreOffice)

- Probe for an installed LibreOffice (`soffice --headless`); RADIAL will not bundle it
- DOCX/ODT/RTF → PDF, DOCX ↔ ODT, HTML → PDF, XLSX/ODS → PDF/CSV
  (DOCX → text and TXT → PDF already ship in v0.2 without LibreOffice)
- Isolated user profile per run (`-env:UserInstallation`), timeouts, clear fidelity caveats

## Later

- macOS build (architecture is already platform-neutral; needs signing/notarisation)
- AVIF and HEIC input/output (licensing and encoder maturity permitting)
- Light theme
