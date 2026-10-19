# Architecture

RADIAL has three layers with one-way dependencies:

```
┌──────────────────────────── src/ (React, TypeScript) ────────────────────────────┐
│ App ─ workflow reducer ─ RadialMenu ─ panels ─ PreviewView                         │
│                    │                                                              │
│              lib/backend.ts  ← the only door to native code                       │
└────────────────────┼──────────────────────────────────────────────────────────────┘
                     │ Tauri IPC (invoke / events)
┌────────────────────┼──────── src-tauri/ (Rust, Tauri 2) ─────────────────────────┐
│ commands.rs  ─ access.rs (path allowlist) ─ state.rs (jobs, queue, caches)        │
│ dialogs (tauri-plugin-dialog) · open/reveal (tauri-plugin-opener) · drag-and-drop │
└────────────────────┼──────────────────────────────────────────────────────────────┘
                     │ plain Rust calls
┌────────────────────┼──────── crates/radial-core/ (Rust, no GUI) ─────────────────┐
│ detect ─ registry ─ options ─ engine ─ naming ─ paths ─ history ─ settings        │
└───────────────────────────────────────────────────────────────────────────────────┘
```

`radial-core` has no Tauri dependency, so all conversion behaviour is tested with
`cargo test -p radial-core` using real files in temporary directories.

## Format detection (`detect.rs`)

Detection never trusts file names.

1. **Sniff** the first 64 bytes for a signature. Supported formats (PNG, JPEG, WebP, BMP, TIFF,
   GIF, PDF) map to `Recognized::Convertible`. Many others (HEIC, AVIF, MP4, MP3, WAV, ZIP/Office,
   SVG, PSD, executables…) map to `Recognized::Known` so the UI can say *"HEIC files are recognised
   but cannot be converted yet"* instead of *"unknown file"*. BMP's weak `BM` signature also
   requires a valid DIB header size. A ZIP is promoted to **Word** when it contains
   `[Content_Types].xml` and `word/document.xml`; **plain text** (no signature) is accepted only
   with a `.txt` extension *and* content that decodes as text.
2. **Parse the header** with the decoder for the sniffed format (not one guessed from the
   extension), under safety limits. This proves the file is *readable* and yields dimensions,
   alpha and bit depth.
3. Compare with the extension only to warn: a PNG named `.jpg` is converted as PNG and flagged.

`FileInfo` keeps these distinctions separate: `recognized` (what it is), `format` (set only
when readable and convertible), and `problem` (why not, as a classified `RadialError`).

## Capability registry (`registry.rs`)

The registry is the single source of truth for what RADIAL can do. Each `Capability` records:

| Field | Meaning |
| --- | --- |
| `from`, `to` | source and destination format |
| `engine`, `engineStatus`, `available` | which engine performs it and whether it is installed |
| `options` | the options this conversion supports (`Quality{min,max,default}`, `Lossless`, `Background`, `Resize`, `Rotate`, `Crop`, `BrightnessContrast`) |
| `limitations` | human-readable caveats shown under "Good to know" |
| `progress` | `Stages` — named stages, never invented percentages |
| `cancellation` | `BetweenStages` — honoured between stages; nothing is written on cancel |

The UI asks `capabilities_for(format)` and renders exactly that list in both the ring and the
accessible list. Options panels render only declared options. `engine::plan` re-checks the registry
before any work, so a request for an unregistered pair is rejected even if a buggy UI sends it.

Today there is one engine, `NativeImage` (always available). External engines will add an
`EngineStatus::Missing { hint }` path from a runtime probe; capabilities with a missing engine are
returned with `available: false`, shown disabled with the hint, and refused by the backend.

## Conversion pipeline (`engine.rs`)

```
plan ─▶ decode ─▶ process ─▶ encode ─▶ verify ─▶ persist
         │          │           │          │          │
   EXIF orient.  rotate→crop→   hidden    re-sniff +  rename into place
   ICC captured  resize→tone→   temp file  full decode without overwrite
                 flatten/convert beside the  check dims  (persist_noclobber)
                 pixel layout    destination
```

- **plan** validates the source, the registry pair, options, file name and destination, and
  refuses to touch the original or an existing file.
- **decode** uses `image` with limits (30 000 px per side, 2 GiB allocation) to stop
  decompression bombs, applies EXIF orientation, and captures the ICC profile.
- **process** applies edits in a fixed order — rotate, crop (fractions of the rotated image),
  resize (Lanczos3), brightness/contrast (lookup tables, alpha untouched, bit depth preserved) —
  then converts to a pixel layout the target accepts, flattening alpha onto the background colour
  for JPEG/BMP.
- **encode** writes into `.radial-XXXX.part` in the destination folder (same volume, so the final
  rename is atomic). JPEG quality uses `image`'s encoder; WebP uses libwebp (lossy or lossless).
  ICC profiles are re-embedded for PNG and JPEG.
- **verify** re-reads the temp file: signature must match the target and a full decode must give
  the expected dimensions.
- **persist** uses `persist_noclobber` — if a file with that name appeared meanwhile, the
  conversion fails with `DestinationExists` and nothing is overwritten.

Cancellation is a shared `AtomicBool` checked between stages; failures and cancellations drop the
temp file, so no half-written output can look finished.

## Native integration (`src-tauri`)

**Path allowlist (`access.rs`).** Commands accept paths only if the user supplied them this
session through the native file dialog, drag-and-drop (captured in Rust from the window's
`DragDrop` event), launch arguments, a folder picker, a saved default folder, or if RADIAL itself
created them. The webview has no file-system, dialog, shell or opener permissions of its own
(`capabilities/default.json`), and each app command is an explicit permission generated by
`build.rs`.

**Jobs.** `convert` registers a cancel flag, emits `radial://job` stage events (`queued`,
`decoding`, `processing`, `encoding`, `verifying`), waits for one of two slots
(`tokio::sync::Semaphore`) and runs the engine on a blocking thread so the UI never stalls.

**Previews.** `preview(path, edits, maxDim)` returns raw JPEG/PNG bytes (`tauri::ipc::Response`),
so non-browser formats like TIFF and BMP preview correctly. Decoded, downscaled bases are cached
(4 entries) so slider changes re-render quickly. The frontend picks the smallest resolution tier
that covers what's on screen.

**History and settings** are JSON files in the per-user app data folder, written atomically.
History stores names, paths, formats, status, error category and options — never file contents —
and is capped at 200 entries.

## Operations (`ops.rs`)

`operations_for(registry, file)` returns all eight actions for a file, each `available` or
disabled with a `reason`, plus its options and limitations. It is the only source the UI uses to
draw the ring. `run(request)` re-checks availability (the backend refuses anything not offered),
sanitises options per action, and dispatches:

- images → `engine.rs` (`image_job`/`encode_job`), `draw.rs` (annotations, redaction boxes),
  `metadata.rs` (lossless EXIF / PNG text edits);
- PDFs → `pdf/` (below);
- Word and text → `doc/docx.rs`, `doc/text.rs`.

Every action that writes a file follows `output.rs`: plan (validate name, never the source, never
an existing file) → stage a hidden temp file beside the destination → verify → no-clobber rename.

## PDF engine (`pdf/`)

- **PDFium** (BSD-3-Clause; pinned build `chromium/7881` from bblanchon/pdfium-binaries, SHA-256
  checked by `scripts/fetch-pdfium.mjs`) is loaded at runtime from the app's `pdfium/` resource
  folder. It renders pages, extracts/searches text and edits page content. If it is missing, PDF
  actions report `engine_missing` instead of failing obscurely.
- **lopdf** (MIT) rewrites the document structure: metadata, compression, and dropping
  unreferenced objects so removed content never lingers.
- Positions from the UI are fractions of the page *as displayed*; `page_rect`/`page_point` map
  them with PDFium's own device transform, so rotated and cropped pages are exact.
- **Redaction** replaces each affected page with a rendered image in which the areas are black,
  deletes every original object and annotation on it, prunes orphaned form fields, thumbnails,
  tags and metadata, rebuilds the file, and verifies: no text on redacted pages, other pages'
  text unchanged, search terms absent from every page and from every string in the file.

## Documents (`doc/`)

- `docx.rs` reads the ZIP package with size limits; text comes from `word/document.xml`
  (paragraphs, tabs, breaks; tracked deletions and field codes skipped). Property edits rewrite
  `docProps/core.xml` (and `app.xml` when stripping) and copy every other entry raw.
- `text.rs` decodes UTF-8/UTF-16 text and typesets it on A4 with PDFium (Courier, or embedded
  DejaVu Sans for non-Latin text), verifying that the PDF contains exactly the source characters.

## Frontend

- `state/workflow.ts` — the workflow as a pure reducer
  (`empty → choose → configure → converting → done | failed`, plus `rejected`). Unit-tested.
- `radial/geometry.ts` — ring layout, sector paths, hit-testing for drag. Unit-tested.
- `radial/RadialMenu.tsx` — SVG ring; segments are `role="radio"` in a `radiogroup` with a roving
  tab stop. Pointer capture drives drag-to-select with a 6 px threshold so clicks stay clicks.
- `lib/backend.ts` — `Backend` interface; `tauriBackend.ts` in the app, `mockBackend.ts` only in
  `--mode mock` builds (excluded from production by a compile-time constant).

## Adding an engine (e.g. FFmpeg)

1. Add an `EngineId` variant and a runtime probe for `status()` (locate the binary; never use a
   shell — pass arguments as a list with `std::process::Command`).
2. Add `Format` variants and signatures (most are already recognised in `detect.rs`).
3. Register capabilities with accurate options and limitations.
4. Implement the handler with the same plan → temp file → verify → persist contract.
5. Add integration tests that run the real engine on real files; skip with a clear message when
   the engine is absent in CI.
