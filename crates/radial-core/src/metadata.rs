//! Reading and editing file metadata without touching image data.
//!
//! - JPEG: the EXIF APP1 segment is rebuilt; every other byte of the file
//!   (including the compressed image) is copied verbatim.
//! - PNG: text chunks are replaced; image chunks are copied verbatim.
//! - Other image formats: read-only.
//!
//! Writes always go to a new file; see [`crate::ops`].

use crate::error::{ErrorKind, RadialError, Result};
use crate::format::Format;
use exif::{Context, Field, In, Tag, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Keys used across formats for the editable fields.
pub const TITLE: &str = "title";
pub const AUTHOR: &str = "author";
pub const SUBJECT: &str = "subject";
pub const KEYWORDS: &str = "keywords";
pub const COPYRIGHT: &str = "copyright";

const MAX_VALUE_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaField {
    pub key: String,
    pub label: String,
    pub value: Option<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetaDetail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataReport {
    pub fields: Vec<MetaField>,
    /// Read-only facts (camera, dates, software…).
    pub details: Vec<MetaDetail>,
    pub can_edit: bool,
    pub can_strip: bool,
    /// The file records a GPS position.
    pub has_location: bool,
    pub notes: Vec<String>,
}

/// Requested changes. Empty strings remove a field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetadataEdit {
    pub fields: BTreeMap<String, String>,
    /// Remove all non-essential metadata (location, camera, text…).
    pub strip_all: bool,
}

fn bad(msg: impl Into<String>) -> RadialError {
    RadialError::new(ErrorKind::InvalidOptions, msg)
}

pub fn supports_editing(format: Format) -> bool {
    matches!(format, Format::Jpeg | Format::Png)
}

impl MetadataEdit {
    pub fn validate(&self, allowed: &[&str]) -> Result<()> {
        for (k, v) in &self.fields {
            if !allowed.contains(&k.as_str()) {
                return Err(bad(format!("“{k}” can't be edited for this file.")));
            }
            if v.chars().count() > MAX_VALUE_CHARS {
                return Err(bad("A metadata value is too long."));
            }
            if v.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
                return Err(bad("Metadata values can't contain control characters."));
            }
        }
        if self.fields.is_empty() && !self.strip_all {
            return Err(bad(
                "Nothing to change: edit a field or choose to remove all metadata.",
            ));
        }
        Ok(())
    }
}

const IMAGE_KEYS: [(&str, &str); 5] = [
    (TITLE, "Title"),
    (AUTHOR, "Author"),
    (SUBJECT, "Subject"),
    (KEYWORDS, "Keywords"),
    (COPYRIGHT, "Copyright"),
];

pub fn editable_keys(format: Format) -> Vec<&'static str> {
    if supports_editing(format) {
        IMAGE_KEYS.iter().map(|(k, _)| *k).collect()
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------- reading

pub fn read_image(path: &Path, format: Format) -> Result<MetadataReport> {
    let data = std::fs::read(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    let mut report = MetadataReport {
        can_edit: supports_editing(format),
        can_strip: supports_editing(format),
        ..Default::default()
    };
    let mut values: BTreeMap<&str, String> = BTreeMap::new();

    let exif = match format {
        Format::Jpeg => jpeg_exif(&data)?,
        Format::Png => {
            png_chunk(&data, b"eXIf")?.and_then(|b| exif::Reader::new().read_raw(b).ok())
        }
        _ => exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(&data))
            .ok(),
    };
    if let Some(ex) = &exif {
        exif_details(ex, &mut report);
        for (key, tag) in [
            (TITLE, Tag::ImageDescription),
            (AUTHOR, Tag::Artist),
            (COPYRIGHT, Tag::Copyright),
        ] {
            if let Some(v) = ascii_field(ex, tag) {
                values.insert(key, v);
            }
        }
        for (key, id) in [
            (TITLE, XP_TITLE),
            (AUTHOR, XP_AUTHOR),
            (SUBJECT, XP_SUBJECT),
            (KEYWORDS, XP_KEYWORDS),
        ] {
            if let Some(v) = xp_field(ex, id) {
                values.insert(key, v);
            }
        }
    }
    if format == Format::Jpeg {
        let segs = jpeg_segments(&data)?;
        if segs
            .segments
            .iter()
            .any(|s| s.marker == 0xE1 && data[s.payload()].starts_with(XMP_SIG))
        {
            report.notes.push("This photo also has an XMP packet (used by Adobe apps). RADIAL edits EXIF; XMP is left as is unless you remove all metadata.".into());
        }
        if segs.segments.iter().any(|s| s.marker == 0xED) {
            report.notes.push(
                "IPTC/Photoshop data is present; it is removed only when you remove all metadata."
                    .into(),
            );
        }
    }
    if format == Format::Png {
        for (k, v) in png_text(&data)? {
            let key = match k.as_str() {
                "Title" => TITLE,
                "Author" => AUTHOR,
                "Description" => SUBJECT,
                "Keywords" => KEYWORDS,
                "Copyright" => COPYRIGHT,
                other => {
                    report.details.push(MetaDetail {
                        label: other.to_string(),
                        value: truncate(&v),
                    });
                    continue;
                }
            };
            values.insert(key, v);
        }
    }
    report.fields = IMAGE_KEYS
        .iter()
        .map(|(k, label)| MetaField {
            key: (*k).into(),
            label: (*label).into(),
            value: values.get(k).cloned(),
            editable: report.can_edit,
        })
        .collect();
    if !report.can_edit {
        report.notes.push(format!(
            "Editing metadata isn't supported for {} yet. Convert to PNG or JPEG to add metadata.",
            format.label()
        ));
    }
    Ok(report)
}

fn truncate(s: &str) -> String {
    if s.chars().count() > 200 {
        format!("{}…", s.chars().take(200).collect::<String>())
    } else {
        s.to_string()
    }
}

const XP_TITLE: u16 = 0x9C9B;
const XP_COMMENT: u16 = 0x9C9C;
const XP_AUTHOR: u16 = 0x9C9D;
const XP_KEYWORDS: u16 = 0x9C9E;
const XP_SUBJECT: u16 = 0x9C9F;

fn ascii_field(ex: &exif::Exif, tag: Tag) -> Option<String> {
    match &ex.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(v) => {
            let s = v
                .iter()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            let s = s.trim().to_string();
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

fn xp_field(ex: &exif::Exif, id: u16) -> Option<String> {
    match &ex.get_field(Tag(Context::Tiff, id), In::PRIMARY)?.value {
        Value::Byte(b) => {
            let units: Vec<u16> = b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            let s = String::from_utf16_lossy(&units)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

fn exif_details(ex: &exif::Exif, report: &mut MetadataReport) {
    let mut add = |label: &str, tag: Tag| {
        if let Some(f) = ex.get_field(tag, In::PRIMARY) {
            let v = f.display_value().with_unit(ex).to_string();
            let v = v.trim_matches('"').trim().to_string();
            if !v.is_empty() {
                report.details.push(MetaDetail {
                    label: label.into(),
                    value: truncate(&v),
                });
            }
        }
    };
    add("Camera make", Tag::Make);
    add("Camera model", Tag::Model);
    add("Lens", Tag::LensModel);
    add("Taken", Tag::DateTimeOriginal);
    add("Exposure", Tag::ExposureTime);
    add("Aperture", Tag::FNumber);
    add("ISO", Tag::PhotographicSensitivity);
    add("Focal length", Tag::FocalLength);
    add("Software", Tag::Software);
    add("Orientation", Tag::Orientation);
    if ex.get_field(Tag::GPSLatitude, In::PRIMARY).is_some() {
        report.has_location = true;
        let lat = ex
            .get_field(Tag::GPSLatitude, In::PRIMARY)
            .map(|f| f.display_value().with_unit(ex).to_string());
        let lon = ex
            .get_field(Tag::GPSLongitude, In::PRIMARY)
            .map(|f| f.display_value().with_unit(ex).to_string());
        report.details.push(MetaDetail {
            label: "GPS location".into(),
            value: format!("{} {}", lat.unwrap_or_default(), lon.unwrap_or_default())
                .trim()
                .to_string(),
        });
    }
    if ex.fields().any(|f| f.tag == Tag::MakerNote) {
        report.details.push(MetaDetail {
            label: "Maker notes".into(),
            value: "Present".into(),
        });
    }
}

// ---------------------------------------------------------------- JPEG

const EXIF_SIG: &[u8] = b"Exif\0\0";
const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(crate) struct Segment {
    marker: u8,
    start: usize,
    end: usize,
}

impl Segment {
    fn payload(&self) -> std::ops::Range<usize> {
        (self.start + 4).min(self.end)..self.end
    }
}

pub(crate) struct JpegLayout {
    segments: Vec<Segment>,
    /// Offset of the SOS marker; everything from here is copied verbatim.
    scan_start: usize,
}

fn corrupt() -> RadialError {
    RadialError::new(ErrorKind::CorruptFile, "The JPEG structure is damaged.")
}

pub(crate) fn jpeg_segments(d: &[u8]) -> Result<JpegLayout> {
    if !d.starts_with(&[0xFF, 0xD8]) {
        return Err(corrupt());
    }
    let mut i = 2;
    let mut segments = Vec::new();
    loop {
        if i + 4 > d.len() || d[i] != 0xFF {
            return Err(corrupt());
        }
        while i + 1 < d.len() && d[i + 1] == 0xFF {
            i += 1; // fill bytes
        }
        let marker = d[i + 1];
        if marker == 0xDA {
            return Ok(JpegLayout {
                segments,
                scan_start: i,
            });
        }
        if marker == 0xD9 {
            return Err(corrupt());
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([d[i + 2], d[i + 3]]));
        let end = i + 2 + len;
        if len < 2 || end > d.len() {
            return Err(corrupt());
        }
        segments.push(Segment {
            marker,
            start: i,
            end,
        });
        i = end;
    }
}

fn jpeg_exif(data: &[u8]) -> Result<Option<exif::Exif>> {
    let layout = jpeg_segments(data)?;
    for s in &layout.segments {
        let p = &data[s.payload()];
        if s.marker == 0xE1 && p.starts_with(EXIF_SIG) {
            return Ok(exif::Reader::new()
                .read_raw(p[EXIF_SIG.len()..].to_vec())
                .ok());
        }
    }
    Ok(None)
}

fn ascii(tag: Tag, v: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![v.as_bytes().to_vec()]),
    }
}

fn xp(id: u16, v: &str) -> Field {
    let mut bytes: Vec<u8> = v.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend_from_slice(&[0, 0]);
    Field {
        tag: Tag(Context::Tiff, id),
        ifd_num: In::PRIMARY,
        value: Value::Byte(bytes),
    }
}

/// Rebuild the EXIF payload with the requested changes. Returns the TIFF
/// bytes (or `None` when nothing remains) and notes for the user.
fn rebuild_exif(
    existing: Option<&exif::Exif>,
    edit: &MetadataEdit,
    include_thumbnail: bool,
) -> Result<(Option<Vec<u8>>, Vec<String>)> {
    let mut notes = Vec::new();
    let edited_tiff = [Tag::ImageDescription, Tag::Artist, Tag::Copyright];
    let edited_xp = [XP_TITLE, XP_AUTHOR, XP_SUBJECT, XP_KEYWORDS, XP_COMMENT];
    let mut fields: Vec<Field> = Vec::new();
    let mut thumbnail: Option<Vec<u8>> = None;
    let little_endian = existing.map(|e| e.little_endian()).unwrap_or(false);

    if let Some(ex) = existing {
        if edit.strip_all {
            // Keep only what's needed to display the image correctly.
            if let Some(f) = ex.get_field(Tag::Orientation, In::PRIMARY) {
                if f.value.get_uint(0).is_some_and(|o| o != 1) {
                    fields.push(f.clone());
                }
            }
        } else {
            let mut dropped_unknown = 0;
            let mut dropped_maker = false;
            for f in ex.fields() {
                if f.ifd_num == In::PRIMARY
                    && (edited_tiff.contains(&f.tag)
                        || edited_xp.iter().any(|id| f.tag == Tag(Context::Tiff, *id)))
                {
                    continue;
                }
                if f.tag == Tag::MakerNote {
                    dropped_maker = true;
                    continue;
                }
                if matches!(f.value, Value::Unknown(..)) {
                    dropped_unknown += 1;
                    continue;
                }
                fields.push(f.clone());
            }
            // Carry the embedded thumbnail over, if any.
            if !include_thumbnail {
                fields.retain(|f| f.ifd_num == In::PRIMARY);
            } else if let (Some(off), Some(len)) = (
                ex.get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
                    .and_then(|f| f.value.get_uint(0)),
                ex.get_field(Tag::JPEGInterchangeFormatLength, In::THUMBNAIL)
                    .and_then(|f| f.value.get_uint(0)),
            ) {
                let (off, len) = (off as usize, len as usize);
                if let Some(t) = ex.buf().get(off..off + len) {
                    thumbnail = Some(t.to_vec());
                }
            }
            if dropped_maker {
                notes.push("Camera maker notes were removed: their internal layout can't be preserved safely when EXIF is rewritten.".into());
            }
            if dropped_unknown > 0 {
                notes.push(format!(
                    "{dropped_unknown} unreadable EXIF entries were removed."
                ));
            }
        }
    }
    if !edit.strip_all {
        for (k, v) in &edit.fields {
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            match k.as_str() {
                TITLE => {
                    fields.push(ascii(Tag::ImageDescription, v));
                    fields.push(xp(XP_TITLE, v));
                }
                AUTHOR => {
                    fields.push(ascii(Tag::Artist, v));
                    fields.push(xp(XP_AUTHOR, v));
                }
                SUBJECT => fields.push(xp(XP_SUBJECT, v)),
                KEYWORDS => fields.push(xp(XP_KEYWORDS, v)),
                COPYRIGHT => fields.push(ascii(Tag::Copyright, v)),
                _ => {}
            }
        }
        // Fields not mentioned in the edit keep their old values.
        if let Some(ex) = existing {
            for (key, tags) in [
                (TITLE, &[Tag::ImageDescription][..]),
                (AUTHOR, &[Tag::Artist][..]),
                (COPYRIGHT, &[Tag::Copyright][..]),
            ] {
                if !edit.fields.contains_key(key) {
                    for t in tags {
                        if let Some(f) = ex.get_field(*t, In::PRIMARY) {
                            fields.push(f.clone());
                        }
                    }
                }
            }
            for (key, id) in [
                (TITLE, XP_TITLE),
                (AUTHOR, XP_AUTHOR),
                (SUBJECT, XP_SUBJECT),
                (KEYWORDS, XP_KEYWORDS),
                ("", XP_COMMENT),
            ] {
                if key.is_empty() || !edit.fields.contains_key(key) {
                    if let Some(f) = ex.get_field(Tag(Context::Tiff, id), In::PRIMARY) {
                        fields.push(f.clone());
                    }
                }
            }
        }
    }
    let has_primary = fields.iter().any(|f| f.ifd_num == In::PRIMARY);
    if !has_primary {
        return Ok((None, notes));
    }
    let mut writer = exif::experimental::Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let has_thumb_ifd = fields.iter().any(|f| f.ifd_num == In::THUMBNAIL);
    if let Some(t) = &thumbnail {
        writer.set_jpeg(t, In::THUMBNAIL);
    } else if has_thumb_ifd {
        // Thumbnail IFD without data: drop those fields by rebuilding without them.
        let primary: Vec<&Field> = fields.iter().filter(|f| f.ifd_num == In::PRIMARY).collect();
        writer = exif::experimental::Writer::new();
        for f in primary {
            writer.push_field(f);
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    writer.write(&mut out, little_endian).map_err(|e| {
        RadialError::new(
            ErrorKind::EngineFailed,
            format!("Writing EXIF failed: {e}."),
        )
    })?;
    Ok((Some(out.into_inner()), notes))
}

pub fn write_jpeg(data: &[u8], edit: &MetadataEdit) -> Result<(Vec<u8>, Vec<String>)> {
    let layout = jpeg_segments(data)?;
    let existing = jpeg_exif(data)?;
    let (mut tiff, mut notes) = rebuild_exif(existing.as_ref(), edit, true)?;
    let too_big = |t: &Option<Vec<u8>>| {
        t.as_ref()
            .is_some_and(|t| t.len() + EXIF_SIG.len() + 2 > 0xFFFF)
    };
    if too_big(&tiff) {
        // Too big for one APP1 segment: retry without the embedded thumbnail.
        let (t, n) = rebuild_exif(existing.as_ref(), edit, false)?;
        tiff = t;
        notes = n;
        notes.push("The embedded thumbnail was too large to keep and was removed.".into());
        if too_big(&tiff) {
            return Err(RadialError::new(
                ErrorKind::EngineFailed,
                "The EXIF data is too large to rewrite.",
            ));
        }
    }
    let app1 = tiff.map(|t| {
        let len = (t.len() + EXIF_SIG.len() + 2) as u16;
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&len.to_be_bytes());
        seg.extend_from_slice(EXIF_SIG);
        seg.extend_from_slice(&t);
        seg
    });

    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut inserted = false;
    let mut removed = 0usize;
    for s in &layout.segments {
        let payload = &data[s.payload()];
        let is_exif = s.marker == 0xE1 && payload.starts_with(EXIF_SIG);
        let keep = if is_exif {
            false
        } else if edit.strip_all {
            // Keep JFIF (APP0, not JFXX thumbnails), ICC/MPF (APP2), Adobe (APP14)
            // and all non-APPn segments (tables, frame header…).
            match s.marker {
                0xE0 => !payload.starts_with(b"JFXX"),
                0xE2 | 0xEE => true,
                0xE1 | 0xE3..=0xED | 0xEF | 0xFE => false,
                _ => true,
            }
        } else {
            true
        };
        if !keep && !is_exif {
            removed += 1;
        }
        // Place the new EXIF where the old one was, or right after APP0/SOI.
        if !inserted && (is_exif || (s.marker != 0xE0)) {
            if let Some(a) = &app1 {
                out.extend_from_slice(a);
            }
            inserted = true;
        }
        if keep {
            out.extend_from_slice(&data[s.start..s.end]);
        }
    }
    if !inserted {
        if let Some(a) = &app1 {
            out.extend_from_slice(a);
        }
    }
    out.extend_from_slice(&data[layout.scan_start..]);
    if edit.strip_all && removed > 0 {
        notes.push(format!(
            "Removed {removed} additional metadata block(s) (XMP, IPTC or comments)."
        ));
    }
    Ok((out, notes))
}

// ---------------------------------------------------------------- PNG

const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

struct Chunk {
    kind: [u8; 4],
    start: usize,
    end: usize,
    data: std::ops::Range<usize>,
}

fn png_chunks(d: &[u8]) -> Result<Vec<Chunk>> {
    if !d.starts_with(PNG_SIG) {
        return Err(RadialError::new(ErrorKind::CorruptFile, "Not a PNG file."));
    }
    let mut i = PNG_SIG.len();
    let mut out = Vec::new();
    while i + 12 <= d.len() {
        let len = u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize;
        let end = i + 12 + len;
        if end > d.len() {
            return Err(RadialError::new(
                ErrorKind::CorruptFile,
                "The PNG structure is damaged.",
            ));
        }
        let kind = [d[i + 4], d[i + 5], d[i + 6], d[i + 7]];
        out.push(Chunk {
            kind,
            start: i,
            end,
            data: i + 8..i + 8 + len,
        });
        i = end;
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(out)
}

fn png_chunk(d: &[u8], kind: &[u8; 4]) -> Result<Option<Vec<u8>>> {
    Ok(png_chunks(d)?
        .into_iter()
        .find(|c| &c.kind == kind)
        .map(|c| d[c.data].to_vec()))
}

/// All text entries (tEXt, zTXt, iTXt) as (keyword, text).
fn png_text(d: &[u8]) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for c in png_chunks(d)? {
        let body = &d[c.data.clone()];
        let Some(nul) = body.iter().position(|b| *b == 0) else {
            continue;
        };
        let keyword = latin1(&body[..nul]);
        let rest = &body[nul + 1..];
        let text = match &c.kind {
            b"tEXt" => Some(latin1(rest)),
            b"zTXt" => rest.get(1..).and_then(inflate).map(|t| latin1(&t)),
            b"iTXt" => itxt_text(rest),
            _ => None,
        };
        if let Some(t) = text {
            out.push((keyword, t));
        }
    }
    Ok(out)
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn inflate(z: &[u8]) -> Option<Vec<u8>> {
    // Bounded to stop decompression bombs hidden in text chunks.
    fdeflate::decompress_to_vec_bounded(z, 1 << 20).ok()
}

fn itxt_text(rest: &[u8]) -> Option<String> {
    let (&compressed, rest) = rest.split_first()?;
    let rest = rest.get(1..)?; // compression method
    let lang_end = rest.iter().position(|b| *b == 0)?;
    let rest = &rest[lang_end + 1..];
    let tr_end = rest.iter().position(|b| *b == 0)?;
    let text = &rest[tr_end + 1..];
    let bytes = if compressed == 1 {
        inflate(text)?
    } else {
        text.to_vec()
    };
    String::from_utf8(bytes).ok()
}

fn png_key(key: &str) -> Option<&'static str> {
    Some(match key {
        TITLE => "Title",
        AUTHOR => "Author",
        SUBJECT => "Description",
        KEYWORDS => "Keywords",
        COPYRIGHT => "Copyright",
        _ => return None,
    })
}

fn chunk_bytes(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(body.len() + 12);
    c.extend_from_slice(&(body.len() as u32).to_be_bytes());
    c.extend_from_slice(kind);
    c.extend_from_slice(body);
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(body);
    c.extend_from_slice(&h.finalize().to_be_bytes());
    c
}

pub fn write_png(data: &[u8], edit: &MetadataEdit) -> Result<(Vec<u8>, Vec<String>)> {
    let chunks = png_chunks(data)?;
    let replaced: Vec<&str> = edit.fields.keys().filter_map(|k| png_key(k)).collect();
    let mut new_text = Vec::new();
    if !edit.strip_all {
        for (k, v) in &edit.fields {
            let (Some(kw), v) = (png_key(k), v.trim()) else {
                continue;
            };
            if v.is_empty() {
                continue;
            }
            let mut body = kw.as_bytes().to_vec();
            body.extend_from_slice(&[0, 0, 0, 0, 0]); // nul, uncompressed, method, empty lang, empty translated
            body.extend_from_slice(v.as_bytes());
            new_text.push(chunk_bytes(b"iTXt", &body));
        }
    }
    let mut out = Vec::with_capacity(data.len() + 256);
    out.extend_from_slice(PNG_SIG);
    let mut inserted = false;
    let mut removed = 0;
    for c in &chunks {
        let is_text = matches!(&c.kind, b"tEXt" | b"zTXt" | b"iTXt");
        let drop = if edit.strip_all {
            is_text || matches!(&c.kind, b"eXIf" | b"tIME")
        } else if is_text {
            let body = &data[c.data.clone()];
            let kw = body
                .iter()
                .position(|b| *b == 0)
                .map(|n| latin1(&body[..n]))
                .unwrap_or_default();
            replaced.contains(&kw.as_str())
        } else {
            false
        };
        if !inserted && &c.kind == b"IDAT" {
            for t in &new_text {
                out.extend_from_slice(t);
            }
            inserted = true;
        }
        if drop {
            removed += 1;
        } else {
            out.extend_from_slice(&data[c.start..c.end]);
        }
    }
    let mut notes = Vec::new();
    if edit.strip_all && removed > 0 {
        notes.push(format!("Removed {removed} metadata chunk(s)."));
    }
    Ok((out, notes))
}

/// Apply `edit` to the bytes of a supported image.
pub fn write_image(
    data: &[u8],
    format: Format,
    edit: &MetadataEdit,
) -> Result<(Vec<u8>, Vec<String>)> {
    edit.validate(&editable_keys(format))?;
    match format {
        Format::Jpeg => write_jpeg(data, edit),
        Format::Png => write_png(data, edit),
        _ => Err(RadialError::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "Editing metadata isn't supported for {} yet.",
                format.label()
            ),
        )),
    }
}

/// Confirm a written file carries the requested values.
pub fn verify_image(path: &Path, format: Format, edit: &MetadataEdit) -> Result<()> {
    let report = read_image(path, format)?;
    let fail = |m: String| RadialError::new(ErrorKind::EngineFailed, m);
    if edit.strip_all {
        if report.has_location {
            return Err(fail("Location data is still present after removal.".into()));
        }
        if report.fields.iter().any(|f| f.value.is_some()) {
            return Err(fail("Some metadata is still present after removal.".into()));
        }
        return Ok(());
    }
    for (k, v) in &edit.fields {
        let got = report
            .fields
            .iter()
            .find(|f| &f.key == k)
            .and_then(|f| f.value.clone());
        let want = (!v.trim().is_empty()).then(|| v.trim().to_string());
        if got != want {
            return Err(fail(format!("The {k} field was not saved correctly.")));
        }
    }
    Ok(())
}
