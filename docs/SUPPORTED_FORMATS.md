# Supported formats and actions

Everything listed here is implemented and exercised by the Rust integration tests in
`crates/radial-core/tests/` (`conversions.rs`, `operations.rs`, `pdf.rs`, `documents.rs`) on real
files. The radial menu is driven by the same registry, so it never offers something not listed here.

## Actions by file type (v0.2)

✓ available · — listed but disabled, with the reason shown in the app.
`operation_matrix_matches_real_support` in `tests/documents.rs` asserts this table and checks that
the backend refuses every disabled action.

| Action | PNG · JPEG · WebP · TIFF | BMP · GIF | PDF | PDF with owner password | Word (.docx) | Text (.txt) |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| Convert | ✓ | ✓ | ✓ PNG, JPEG, Text | — | ✓ Text | ✓ PDF |
| Compress | ✓ | — (no useful compression) | ✓ | — | — | — |
| Edit (rotate, resize, tone) | ✓ | ✓ | — (text/layout editing not supported) | — | — | — |
| Crop | ✓ | ✓ | ✓ one page or all | — | — | — |
| Annotate | ✓ | ✓ | ✓ | — | — | — |
| Redact | ✓ | ✓ | ✓ areas + text search | — | — | — |
| Background | ✓ | ✓ | — | — | — | — |
| Metadata | ✓ view; edit JPEG/PNG | ✓ view | ✓ view/edit/remove | view only | ✓ view/edit/remove | — |

Password-protected PDFs (a password is needed to open them) are recognised and refused with an
explanation. Digitally signed PDFs can be processed, but RADIAL warns that a changed copy won't
carry a valid signature, and doesn't edit their properties.

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

## Image actions

- **Compress**: JPEG/WebP re-encode at the chosen quality; PNG is optimised losslessly (oxipng);
  TIFF is LZW-compressed; optionally limit the longest side. A result that isn't smaller is
  discarded and reported — never saved.
- **Redact**: marked areas become solid black pixels and **all** metadata (EXIF, GPS, thumbnails,
  text chunks) is removed. Nothing under a box exists in the saved file.
- **Annotate**: arrows, lines, boxes, ellipses and text are drawn into the pixels (DejaVu Sans;
  right-to-left scripts are not shaped yet).
- **Metadata**: title/author/subject/keywords/copyright can be edited losslessly in JPEG (EXIF)
  and PNG (iTXt); "remove all" keeps only the orientation. Other formats are view-only.

## PDF (PDFium + lopdf)

| Action | What happens | Limits |
| --- | --- | --- |
| Convert → PNG / JPEG | Renders one chosen page at 72/150/300 DPI (36–600 accepted). | One page per export. |
| Convert → Text | Text of every page, pages separated by a blank line. | No OCR: scanned pages give nothing; layout and tables aren't kept. |
| Compress | Re-encodes JPEG photos at the chosen quality (optionally smaller), compresses streams, removes unused objects and thumbnails, packs object streams. | Text and vector graphics unchanged; refused if not smaller or if any page's text changed. |
| Crop | Sets the crop box of the shown page or all pages (rotation-aware). | Hidden content stays in the file — use Redact for sensitive content. |
| Annotate | Shapes and text drawn into the page content (vector), upright on rotated pages. | Flattened: not editable as comments. Non-Latin text embeds DejaVu Sans (~0.7 MB). |
| Redact | Each page with a redaction is replaced by a 200 DPI image of itself with the areas blacked out; old content, annotations, form fields on those pages, thumbnails, tags and metadata are removed, and the file is rebuilt without unused objects. Text search finds words on every page. | Redacted pages are no longer searchable. Text found only in bookmarks, form values or attachment names is reported and the redaction is refused. |
| Metadata | View; edit title/author/subject/keywords; remove Info and XMP. | Dates/producer read-only; no copyright field in PDF. |

## Documents

| Conversion | How | Limits |
| --- | --- | --- |
| Word (.docx) → Text | Reads `word/document.xml`: paragraphs, tabs, line breaks, table cells. | No formatting, images, headers/footers, footnotes or comments. Tracked deletions excluded. |
| Text (.txt) → PDF | A4, 10 pt; Courier for Latin text (columns stay aligned), embedded DejaVu Sans otherwise; long lines wrap; form feeds start a page. | Markup isn't interpreted. Up to 20 MB of UTF-8 or UTF-16 (with BOM). |

Word properties (title, author, subject, keywords) can be edited; "remove all" also clears
"last modified by", dates, company and manager. Only `docProps/core.xml`/`app.xml` are rewritten —
every other part is copied byte for byte. Custom properties and comments are not changed.

**Not offered:** Word → PDF and PDF → Word. Both need a full layout engine (LibreOffice or Word)
to be faithful; RADIAL doesn't bundle one and won't claim a lossless document conversion.

## Recognised but not supported yet

RADIAL identifies these by content and explains that they can't be processed yet:
other Office documents (XLSX/PPTX as ZIP, legacy .doc/.xls/.ppt), HTML, SVG, HEIC, AVIF, ICO,
PSD, MP3, WAV, FLAC, Ogg, M4A, MP4, MOV, WebM/Matroska, AVI. Executables are identified and refused.
Plain text is accepted only with a `.txt` extension (text has no signature).

Audio/video (FFmpeg) and Office layout conversion (LibreOffice) remain on the
[roadmap](../ROADMAP.md); they will appear in the ring only when implemented and tested.
