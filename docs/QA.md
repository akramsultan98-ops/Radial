# QA report — v0.1.0

Date: 2026-10-18. Environment: Linux x86-64 container (Ubuntu 24.04), Rust 1.97, Node 22.22,
WebKitGTK 2.52, Xvfb + openbox. **No Windows machine was available**; see *Not yet verified*.

## Automated results

| Suite | Command | Result |
| --- | --- | --- |
| Rust unit tests (core) | `cargo test -p radial-core --lib` | 29 passed |
| Rust integration tests — real files, real encoders | `cargo test -p radial-core --test conversions` | 22 passed |
| Rust shell tests (path allowlist) | `cargo test -p radial` | 1 passed |
| Clippy, warnings as errors | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| TypeScript strict | `npm run typecheck` | clean |
| ESLint | `npm run lint` | clean |
| Frontend unit + component tests | `npm test` | 42 passed |
| UI tests (Playwright, mock backend) | `npm run test:ui` | 26 passed |

The integration suite converts all 30 registered pairs and checks content type and dimensions,
plus transparency, background flattening, quality, resize, edits, EXIF orientation, ICC
preservation, 16-bit input, TIFF tags/compression, collisions, cancellation at every stage,
Unicode paths, content-vs-extension detection, invalid/empty/truncated/corrupt files,
decompression-bomb headers, output permissions and read-only sources.

The Playwright suite verifies interaction only (it uses the mock backend). It covers the empty
state, drag-over, import, radial menu, hover, click, drag-to-select, accidental drags, keyboard
navigation, the list alternative, staged progress, success and preview controls, name collisions,
write-time collisions, engine failure + retry, unwritable folder recovery, cancellation, rejected
files, multi-file drops, editing (rotate, brightness, crop, reset), resize with aspect lock,
history (record, clear, retry), settings, dialog focus handling, and layout at 880×620 and
1600×1000. Screenshots: `qa/screenshots/ui/`.

## Native desktop QA (real app, real conversions)

The debug build (`npm run tauri build -- --debug --no-bundle`) was launched under Xvfb and
driven with real pointer and keyboard events (xdotool). Screenshots: `qa/screenshots/native/`.

| # | Check | Result |
| --- | --- | --- |
| 1 | Launch with a file argument in a folder named `In ✓ 写真`, file `Sunset — 夕焼け.png` (16-bit RGBA) | ✅ detected PNG, 1600×1000, 16-bit, transparency; thumbnail rendered by the native preview pipeline (`01`) |
| 2 | Drag the file from the orb onto the JPEG segment with a real mouse drag | ✅ JPEG selected on release, options switched to quality + background (`03`) |
| 3 | Convert to JPEG | ✅ `Sunset — 夕焼け.jpg` 1600×1000, quality 90, transparent corners flattened to white; stage UI shown (`04`, `05`) |
| 4 | Convert to WebP (click segment, `Ctrl+Enter`) | ✅ 1600×1000 WebP, corner alpha 0 / centre alpha 1 (transparency kept) (`06`, `07`) |
| 5 | Rotate right, convert to TIFF | ✅ 1000×1600, 16-bit RGBA, LZW, read by ImageMagick without warnings (`08`, `09`) |
| 6 | History persisted to the app data folder; drawer lists all three | ✅ (`12`) |
| 7 | Launch with a non-image file named `broken.png` | ✅ "This file can't be converted — RADIAL could not recognise this file's contents"; no ring shown (`10`) |
| 8 | Resize window to the 880×620 minimum (with a window manager) | ✅ ring scales down, step labels collapse, nothing clipped (`11`) |
| 9 | Output file permissions | ❌→✅ first run produced `0600` files (temp-file default); fixed to honour the umask, regression test added |
| 10 | TIFF output compatibility | ❌→✅ first run wrote uncompressed RGBA TIFF without `ExtraSamples` (ImageMagick warned); replaced with an LZW writer that tags alpha, test added |

Defects found in native QA and fixed: the two above, a horizontal scrollbar in the side panel
(WebKit range-input margins), a GTK tooltip that followed the pointer during drags, and Escape not
closing the history drawer after its focused button disappeared.

Also verified with `cargo run -p radial-core --example convert`: TIFF inputs in CMYK, JPEG-in-TIFF,
CCITT G4, Deflate, PackBits, LZW, greyscale, 16-bit and multi-page (first page); animated GIF and
animated WebP (first frame).

## Not yet verified

- **Windows.** Nothing has been run on Windows 10/11 yet: installers, WebView2 rendering, drag
  and drop from Explorer, `Open`/`Show in folder` with Explorer, paths with drive letters and
  reserved names, and SmartScreen behaviour. The CI `Windows build` job compiles the app, runs the
  core tests on Windows and produces the installers; manual verification on a Windows machine is
  the next QA step (checklist in `docs/PACKAGING.md`).
- **Native drag-and-drop from a file manager** (only the launch-argument path and the picker
  plumbing were exercised natively; the drop handler shares the same import code).
- **Native file dialogs** (`Choose file`, `Change…` folder) — they open through the dialog
  plugin but were not driven in Xvfb.
- **Mid-drag rendering in Xvfb**: screenshots taken during a drag showed a stale frame (software
  rendering); the drag outcome was correct. The armed-segment visuals are verified in Chromium
  (`qa/screenshots/ui/06-drag-armed.png`).
- **Very large images** (>100 MP) for memory and responsiveness on typical hardware.
- **Screen readers** (Narrator/NVDA) — semantics are in place and tested via ARIA roles, but no
  manual screen-reader pass has been done.
- The reference video mentioned in the project brief was not available in this environment, so
  the interaction design follows the written specification only.
