//! Word documents (Office Open XML): text extraction and document
//! properties (`docProps/core.xml`, `docProps/app.xml`).
//!
//! Property edits rewrite only those two parts; every other part of the
//! package is copied byte-for-byte (still compressed) into the new file.

use crate::error::{ErrorKind, RadialError, Result};
use crate::metadata::{
    MetaDetail, MetaField, MetadataEdit, MetadataReport, AUTHOR, KEYWORDS, SUBJECT, TITLE,
};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

const DOCUMENT: &str = "word/document.xml";
const CORE: &str = "docProps/core.xml";
const APP: &str = "docProps/app.xml";
/// Largest XML part RADIAL will decompress.
const MAX_PART: u64 = 256 * 1024 * 1024;

fn corrupt(m: impl Into<String>) -> RadialError {
    RadialError::new(ErrorKind::CorruptFile, m)
}

type Archive<R> = zip::ZipArchive<R>;

fn open_archive<R: Read + Seek>(r: R) -> Result<Archive<R>> {
    zip::ZipArchive::new(r).map_err(|_| corrupt("The Word document's package is damaged."))
}

fn read_part<R: Read + Seek>(a: &mut Archive<R>, name: &str) -> Result<Option<Vec<u8>>> {
    let mut f = match a.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(corrupt("The Word document's package is damaged.")),
    };
    if f.size() > MAX_PART {
        return Err(RadialError::new(
            ErrorKind::TooLarge,
            "A part of this Word document is too large to open.",
        ));
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.by_ref()
        .take(MAX_PART + 1)
        .read_to_end(&mut out)
        .map_err(|_| corrupt("The Word document's package is damaged."))?;
    Ok(Some(out))
}

/// Whether a ZIP file is a Word document.
pub fn probe(path: &Path) -> bool {
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(a) = zip::ZipArchive::new(std::io::BufReader::new(f)) else {
        return false;
    };
    let has = |n: &str| a.index_for_name(n).is_some();
    has("[Content_Types].xml") && has(DOCUMENT)
}

fn archive_at(path: &Path) -> Result<Archive<std::io::BufReader<std::fs::File>>> {
    let f = std::fs::File::open(path).map_err(|e| RadialError::from_io(&e, "opening the file"))?;
    open_archive(std::io::BufReader::new(f))
}

/// Resolve `&amp;`-style and numeric references.
fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let n = name.strip_prefix('#')?;
            let code = match n.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Text of the main document: paragraphs on their own lines, tabs and line
/// breaks kept, deleted (tracked) text and field codes left out.
pub fn text_of(xml: &[u8]) -> Result<String> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_text = 0u32;
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == "t" => in_text += 1,
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "t" => in_text = in_text.saturating_sub(1),
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                "tab" => out.push('\t'),
                "br" | "cr" => out.push('\n'),
                "p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text > 0 => out.push_str(&t.xml10_content()),
            Ok(Event::CData(t)) if in_text > 0 => out.push_str(&t.into_inner()),
            Ok(Event::GeneralRef(g)) if in_text > 0 => {
                if let Some(c) = entity(&g.xml10_content()) {
                    out.push(c);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(corrupt("The Word document's text is damaged.")),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// Extract the document's text. Fails when there is none.
pub fn to_text(path: &Path) -> Result<String> {
    let mut a = archive_at(path)?;
    let xml =
        read_part(&mut a, DOCUMENT)?.ok_or_else(|| corrupt("The document body is missing."))?;
    let text = text_of(&xml)?;
    let text = text.trim_end().to_string();
    if text.trim().is_empty() {
        return Err(RadialError::new(
            ErrorKind::UnsupportedConversion,
            "This Word document has no text in its body.",
        ));
    }
    Ok(text + "\n")
}

// ---------------------------------------------------------------- properties

/// Elements of core.xml RADIAL understands, as (local name, prefix).
const CORE_ELEMENTS: [(&str, &str); 15] = [
    ("title", "dc"),
    ("subject", "dc"),
    ("creator", "dc"),
    ("keywords", "cp"),
    ("description", "dc"),
    ("lastModifiedBy", "cp"),
    ("revision", "cp"),
    ("lastPrinted", "cp"),
    ("created", "dcterms"),
    ("modified", "dcterms"),
    ("category", "cp"),
    ("contentStatus", "cp"),
    ("language", "dc"),
    ("identifier", "dc"),
    ("version", "cp"),
];

/// Editable keys → core.xml element.
const FIELDS: [(&str, &str, &str); 4] = [
    (TITLE, "Title", "title"),
    (AUTHOR, "Author", "creator"),
    (SUBJECT, "Subject", "subject"),
    (KEYWORDS, "Keywords", "keywords"),
];

pub fn editable_keys() -> Vec<&'static str> {
    FIELDS.iter().map(|(k, _, _)| *k).collect()
}

/// Simple element → text map of a flat XML part (core.xml, app.xml).
fn flat_values(xml: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut values = BTreeMap::new();
    let mut depth = 0usize;
    let mut current: Option<String> = None;
    let mut text = String::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                if depth == 2 {
                    current = Some(e.local_name().as_ref().to_string());
                    text.clear();
                }
            }
            Ok(Event::End(_)) => {
                if depth == 2 {
                    if let Some(k) = current.take() {
                        let v = text.trim();
                        if !v.is_empty() {
                            values.insert(k, v.to_string());
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Text(t)) if depth == 2 => text.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(g)) if depth == 2 => {
                if let Some(c) = entity(&g.xml10_content()) {
                    text.push(c);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(corrupt("The document properties are damaged.")),
            _ => {}
        }
        buf.clear();
    }
    Ok(values)
}

fn date(v: &str) -> String {
    // 2024-01-31T14:30:00Z → 2024-01-31 14:30
    let v = v.trim();
    if v.len() >= 16 && v.as_bytes()[10] == b'T' {
        format!("{} {}", &v[..10], &v[11..16])
    } else {
        v.to_string()
    }
}

pub fn read_metadata(path: &Path) -> Result<MetadataReport> {
    let mut a = archive_at(path)?;
    let core = read_part(&mut a, CORE)?;
    let app = read_part(&mut a, APP)?;
    let core_vals = match &core {
        Some(x) => flat_values(x)?,
        None => BTreeMap::new(),
    };
    let app_vals = match &app {
        Some(x) => flat_values(x)?,
        None => BTreeMap::new(),
    };
    let editable = core.is_some();
    let mut report = MetadataReport {
        can_edit: editable,
        can_strip: editable,
        ..Default::default()
    };
    for (key, label, el) in FIELDS {
        report.fields.push(MetaField {
            key: key.into(),
            label: label.into(),
            value: core_vals.get(el).cloned(),
            editable,
        });
    }
    for (label, v) in [
        ("Last modified by", core_vals.get("lastModifiedBy").cloned()),
        ("Company", app_vals.get("Company").cloned()),
        ("Manager", app_vals.get("Manager").cloned()),
        ("Description", core_vals.get("description").cloned()),
        ("Created", core_vals.get("created").map(|d| date(d))),
        ("Modified", core_vals.get("modified").map(|d| date(d))),
        ("Revision", core_vals.get("revision").cloned()),
        ("Created with", app_vals.get("Application").cloned()),
        ("Pages", app_vals.get("Pages").cloned()),
        ("Words", app_vals.get("Words").cloned()),
    ] {
        if let Some(value) = v {
            report.details.push(MetaDetail {
                label: label.into(),
                value,
            });
        }
    }
    if !editable {
        report
            .notes
            .push("This document has no properties part, so there is nothing to edit.".into());
    }
    report.notes.push(
        "“Last modified by”, company and dates are read-only here; “Remove all” clears them. Custom properties and comments are not changed."
            .into(),
    );
    Ok(report)
}

fn core_xml(values: &BTreeMap<String, String>) -> Vec<u8> {
    let mut w = Writer::new(Vec::new());
    let mut root = BytesStart::new("cp:coreProperties");
    for (k, v) in [
        (
            "xmlns:cp",
            "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        ),
        ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
        ("xmlns:dcterms", "http://purl.org/dc/terms/"),
        ("xmlns:dcmitype", "http://purl.org/dc/dcmitype/"),
        ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    ] {
        root.push_attribute((k, v));
    }
    let mut out = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#.to_vec();
    out.extend_from_slice(b"\r\n");
    let _ = w.write_event(Event::Start(root));
    for (local, prefix) in CORE_ELEMENTS {
        let Some(v) = values.get(local) else { continue };
        let name = format!("{prefix}:{local}");
        let mut start = BytesStart::new(name.as_str());
        if prefix == "dcterms" {
            start.push_attribute(("xsi:type", "dcterms:W3CDTF"));
        }
        let _ = w.write_event(Event::Start(start));
        let _ = w.write_event(Event::Text(BytesText::new(v)));
        let _ = w.write_event(Event::End(quick_xml::events::BytesEnd::new(name.as_str())));
    }
    let _ = w.write_event(Event::End(quick_xml::events::BytesEnd::new(
        "cp:coreProperties",
    )));
    out.extend_from_slice(&w.into_inner());
    out
}

/// Copy app.xml without the Company and Manager elements.
fn strip_app(xml: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::from_reader(xml);
    let mut w = Writer::new(Vec::new());
    let mut buf = Vec::new();
    let mut skip = 0u32;
    loop {
        let ev = r
            .read_event_into(&mut buf)
            .map_err(|_| corrupt("The document properties are damaged."))?;
        let personal = |n: &str| n == "Company" || n == "Manager";
        match &ev {
            Event::Eof => break,
            Event::Start(e) if skip > 0 || personal(e.local_name().as_ref()) => {
                skip += 1;
            }
            Event::End(_) if skip > 0 => skip -= 1,
            Event::Empty(e) if personal(e.local_name().as_ref()) => {}
            _ if skip > 0 => {}
            _ => {
                w.write_event(ev.borrow())
                    .map_err(|e| RadialError::from_io(&e, "writing the document"))?;
            }
        }
        buf.clear();
    }
    Ok(w.into_inner())
}

/// Apply `edit` to a .docx, returning the new package.
pub fn write_metadata(data: &[u8], edit: &MetadataEdit) -> Result<(Vec<u8>, Vec<String>)> {
    edit.validate(&editable_keys())?;
    let mut a = open_archive(Cursor::new(data))?;
    let core = read_part(&mut a, CORE)?.ok_or_else(|| {
        RadialError::new(
            ErrorKind::UnsupportedFormat,
            "This document has no properties part, so RADIAL can't edit it.",
        )
    })?;
    let mut values = flat_values(&core)?;
    let mut notes = Vec::new();
    let new_app = if edit.strip_all {
        values.clear();
        notes.push("Title, author, “last modified by”, dates and other document properties were removed, as were company and manager.".into());
        match read_part(&mut a, APP)? {
            Some(x) => Some(strip_app(&x)?),
            None => None,
        }
    } else {
        for (key, _, el) in FIELDS {
            if let Some(v) = edit.fields.get(key) {
                let v = v.trim();
                if v.is_empty() {
                    values.remove(el);
                } else {
                    values.insert(el.to_string(), v.to_string());
                }
            }
        }
        notes.push("Only the document properties changed; the content is copied unchanged.".into());
        None
    };
    let new_core = core_xml(&values);

    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for i in 0..a.len() {
        let f = a
            .by_index_raw(i)
            .map_err(|_| corrupt("The Word document's package is damaged."))?;
        let name = f
            .name()
            .map_err(|_| corrupt("The Word document's package is damaged."))?
            .to_string();
        let replacement = match name.as_str() {
            CORE => Some(&new_core),
            APP => new_app.as_ref(),
            _ => None,
        };
        let err = |_| RadialError::new(ErrorKind::EngineFailed, "Writing the document failed.");
        match replacement {
            Some(bytes) => {
                drop(f);
                w.start_file(name.as_str(), opts).map_err(err)?;
                w.write_all(bytes)
                    .map_err(|e| RadialError::from_io(&e, "writing the document"))?;
            }
            None => w.raw_copy_file(f).map_err(err)?,
        }
    }
    let out = w
        .finish()
        .map_err(|_| RadialError::new(ErrorKind::EngineFailed, "Writing the document failed."))?
        .into_inner();
    Ok((out, notes))
}

/// Check a written package: every part decompresses, the body text is
/// unchanged, and the properties carry the requested values.
pub fn verify(original: &[u8], written: &[u8], edit: &MetadataEdit) -> Result<()> {
    let fail = |m: &str| RadialError::new(ErrorKind::EngineFailed, m.to_string());
    let mut a = open_archive(Cursor::new(written))?;
    for i in 0..a.len() {
        let mut f = a
            .by_index(i)
            .map_err(|_| fail("The new document is damaged."))?;
        std::io::copy(&mut f, &mut std::io::sink())
            .map_err(|_| fail("A part of the new document is damaged."))?;
    }
    let mut src = open_archive(Cursor::new(original))?;
    let before = read_part(&mut src, DOCUMENT)?;
    let after = read_part(&mut a, DOCUMENT)?;
    if before != after || after.is_none() {
        return Err(fail("The document body changed while editing properties."));
    }
    let core = read_part(&mut a, CORE)?.ok_or_else(|| fail("Properties are missing."))?;
    let values = flat_values(&core)?;
    if edit.strip_all {
        if !values.is_empty() {
            return Err(fail("Some properties are still present after removal."));
        }
        return Ok(());
    }
    for (key, _, el) in FIELDS {
        if let Some(v) = edit.fields.get(key) {
            let want = (!v.trim().is_empty()).then(|| v.trim().to_string());
            if values.get(el).cloned() != want {
                return Err(fail(&format!("The {key} field was not saved correctly.")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_paragraphs_tabs_and_entities_but_not_deletions() {
        let xml = br#"<w:document xmlns:w="w"><w:body>
            <w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">A &amp; B </w:t></w:r></w:p>
            <w:p><w:del><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:t>kept&#233;</w:t><w:br/><w:t>next</w:t></w:r></w:p>
            <w:p><w:r><w:instrText>PAGE</w:instrText></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(text_of(xml).unwrap(), "Hello\tA & B \nkepté\nnext\n\n");
    }

    #[test]
    fn core_xml_round_trips() {
        let mut v = BTreeMap::new();
        v.insert("title".to_string(), "Q3 <draft> & notes".to_string());
        v.insert("created".to_string(), "2024-01-31T14:30:00Z".to_string());
        let xml = core_xml(&v);
        assert_eq!(flat_values(&xml).unwrap(), v);
    }
}
