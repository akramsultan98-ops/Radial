# Supported formats

Every conversion listed here is implemented by the built-in image engine and exercised by
`crates/radial-core/tests/conversions.rs` (`every_registered_conversion_produces_a_verified_file`
converts all 30 pairs and verifies the output's content type and dimensions).

## Image conversions (v0.1)

Any source in the left column converts to any *other* format in the top row.

| from \ to | PNG | JPEG | WebP | BMP | TIFF | GIF |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| **PNG**  | — | ✓ | ✓ | ✓ | ✓ | ✓ |
| **JPEG** | ✓ | — | ✓ | ✓ | ✓ | ✓ |
| **WebP** | ✓ | ✓ | — | ✓ | ✓ | ✓ |
| **BMP**  | ✓ | ✓ | ✓ | — | ✓ | ✓ |
| **TIFF** | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| **GIF**  | ✓ | ✓ | ✓ | ✓ | ✓ | — |

## Options by target

| Target | Quality | Lossless | Background (for transparency) | Resize · rotate · crop · brightness/contrast |
| --- | :-: | :-: | :-: | :-: |
| PNG  |   |   |   | ✓ |
| JPEG | ✓ (1–100, default 90) |   | ✓ | ✓ |
| WebP | ✓ (1–100, default 85) | ✓ |   | ✓ |
| BMP  |   |   | ✓ | ✓ |
| TIFF |   |   |   | ✓ |
| GIF  |   |   |   | ✓ |

## Behaviour and known limitations

- **Orientation**: EXIF orientation is applied to the pixels; EXIF/XMP metadata (camera, GPS) is
  not copied — a deliberate privacy-friendly default.
- **Transparency** is preserved for PNG, WebP (lossy and lossless), TIFF and GIF. JPEG and BMP
  have no reliable alpha, so transparent areas are composited onto the background colour
  (default white).
- **Bit depth**: 16-bit sources stay 16-bit in PNG and TIFF; JPEG, WebP, BMP and GIF are 8-bit.
- **Colour profiles**: embedded ICC profiles are carried into PNG and JPEG output. WebP, TIFF and
  BMP output does not embed the profile; pixel values are written unchanged.
- **TIFF output** is LZW-compressed (lossless) with horizontal prediction, and RGBA files carry an
  `ExtraSamples = unassociated alpha` tag so other applications read transparency correctly.
- **GIF**: only the first frame of an animated GIF is converted; GIF output is a single frame
  quantised to 256 colours with on/off transparency.
- **Animated WebP input**: like GIF, only the first frame is converted.
- **TIFF input**: verified with 8/16-bit RGB, greyscale, CMYK (naïve conversion, no ICC-based
  colour management), uncompressed, LZW, Deflate, PackBits, JPEG-in-TIFF and CCITT Group 4.
  Multi-page TIFFs convert the **first page** only. Variants the decoder can't read are reported
  as unsupported or damaged rather than converted wrongly.
- **Size limits**: files up to 1 GB and images up to 30 000 px per side; larger inputs are
  rejected before decoding to protect memory.

## Recognised but not convertible yet

RADIAL identifies these by content and explains that they can't be converted yet:
PDF, Office documents (DOCX/XLSX/PPTX as ZIP, legacy OLE), HTML, SVG, HEIC, AVIF, ICO, PSD,
MP3, WAV, FLAC, Ogg, M4A, MP4, MOV, WebM/Matroska, AVI. Executables are identified and refused.

Document conversion (LibreOffice) and audio/video conversion (FFmpeg) are on the
[roadmap](../ROADMAP.md). They will appear in the ring only when the engine is installed and
the specific conversion is implemented and tested.
