//! PDF document metadata: the Info dictionary (title, author, subject,
//! keywords, creator, producer, dates) and the XMP metadata stream.

use super::{engine, load_lopdf, open, save_lopdf};
use crate::error::{ErrorKind, RadialError, Result};
use crate::metadata::{
    MetaDetail, MetaField, MetadataEdit, MetadataReport, AUTHOR, KEYWORDS, SUBJECT, TITLE,
};
use lopdf::{Dictionary, Object, StringFormat};
use std::path::Path;

/// Editable keys and their Info dictionary names.
const FIELDS: [(&str, &str, &[u8]); 4] = [
    (TITLE, "Title", b"Title"),
    (AUTHOR, "Author", b"Author"),
    (SUBJECT, "Subject", b"Subject"),
    (KEYWORDS, "Keywords", b"Keywords"),
];

pub fn editable_keys() -> Vec<&'static str> {
    FIELDS.iter().map(|(k, _, _)| *k).collect()
}

/// Decode a PDF text string (UTF-16BE with BOM, UTF-8 with BOM, or
/// PDFDocEncoding, treated as Latin-1).
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    }
}

/// Encode text for the Info dictionary: plain ASCII stays readable,
/// anything else is UTF-16BE.
fn encode_text(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut b = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(b, StringFormat::Hexadecimal)
    }
}

/// "D:20240131143000+01'00'" → "2024-01-31 14:30".
fn pdf_date(s: &str) -> String {
    let d = s.trim().trim_start_matches("D:");
    let digits: String = d.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 8 {
        return s.trim().to_string();
    }
    let mut out = format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8]);
    if digits.len() >= 12 {
        out.push_str(&format!(" {}:{}", &digits[8..10], &digits[10..12]));
    }
    out
}

fn info_dict(doc: &lopdf::Document) -> Option<&Dictionary> {
    match doc.trailer.get(b"Info").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn info_value(doc: &lopdf::Document, d: &Dictionary, key: &[u8]) -> Option<String> {
    let v = d.get(key).ok()?;
    let (_, v) = doc.dereference(v).ok()?;
    match v {
        Object::String(b, _) => {
            let s = decode_text(b).trim().to_string();
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

pub fn read_metadata(path: &Path) -> Result<MetadataReport> {
    let info = super::inspect(path)?;
    let mut report = MetadataReport::default();
    let editable = !info.restricted && !info.signed;
    report.can_edit = editable;
    report.can_strip = editable;

    let data = std::fs::read(path).map_err(|e| RadialError::from_io(&e, "reading the file"))?;
    let mut values = std::collections::BTreeMap::new();
    let mut details = Vec::new();
    match load_lopdf(&data) {
        Ok(doc) if !doc.is_encrypted() => {
            if let Some(d) = info_dict(&doc) {
                for (key, _, name) in FIELDS {
                    if let Some(v) = info_value(&doc, d, name) {
                        values.insert(key, v);
                    }
                }
                for (label, name, date) in [
                    ("Created with", &b"Creator"[..], false),
                    ("PDF producer", b"Producer", false),
                    ("Created", b"CreationDate", true),
                    ("Modified", b"ModDate", true),
                ] {
                    if let Some(v) = info_value(&doc, d, name) {
                        details.push(MetaDetail {
                            label: label.into(),
                            value: if date { pdf_date(&v) } else { v },
                        });
                    }
                }
            }
            if doc.catalog().is_ok_and(|c| c.has(b"Metadata")) {
                details.push(MetaDetail {
                    label: "XMP metadata".into(),
                    value: "Present".into(),
                });
            }
            details.push(MetaDetail {
                label: "PDF version".into(),
                value: doc.version.clone(),
            });
        }
        _ => {
            // Encrypted or unusual files: read what PDFium can decrypt.
            let pdfium = engine()?;
            let pdoc = open(pdfium, path)?;
            use pdfium_render::prelude::PdfDocumentMetadataTagType as T;
            for (key, tag) in [
                (TITLE, T::Title),
                (AUTHOR, T::Author),
                (SUBJECT, T::Subject),
                (KEYWORDS, T::Keywords),
            ] {
                if let Some(v) = pdoc.metadata().get(tag) {
                    let v = v.value().trim().to_string();
                    if !v.is_empty() {
                        values.insert(key, v);
                    }
                }
            }
        }
    }
    for (key, label, _) in FIELDS {
        report.fields.push(MetaField {
            key: key.into(),
            label: label.into(),
            value: values.remove(key),
            editable,
        });
    }
    details.insert(
        0,
        MetaDetail {
            label: "Pages".into(),
            value: info.pages.to_string(),
        },
    );
    details.insert(
        1,
        MetaDetail {
            label: "Page size".into(),
            value: format!(
                "{:.0} × {:.0} mm",
                info.width / 72.0 * 25.4,
                info.height / 72.0 * 25.4
            ),
        },
    );
    report.details = details;
    if info.restricted {
        report
            .notes
            .push("This PDF has security restrictions, so RADIAL won't change it.".into());
    }
    if info.signed {
        report.notes.push(
            "This PDF is digitally signed. Changing it would invalidate the signature, so editing is turned off."
                .into(),
        );
    }
    report.notes.push(
        "PDFs have no standard copyright field. Created/modified dates and the producer are shown but not edited."
            .into(),
    );
    Ok(report)
}

/// Remove the Info dictionary, the document XMP stream and per-page XMP.
pub(crate) fn strip_document_metadata(doc: &mut lopdf::Document) {
    doc.trailer.remove(b"Info");
    if let Ok(c) = doc.catalog_mut() {
        c.remove(b"Metadata");
    }
    let ids: Vec<_> = doc.get_pages().values().copied().collect();
    for id in ids {
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.remove(b"Metadata");
            d.remove(b"PieceInfo");
        }
    }
}

/// Apply `edit` to a PDF's bytes. Pages and content are untouched.
pub fn write_metadata(data: &[u8], edit: &MetadataEdit) -> Result<(Vec<u8>, Vec<String>)> {
    edit.validate(&editable_keys())?;
    let mut doc = load_lopdf(data)?;
    if doc.is_encrypted() {
        return Err(RadialError::new(
            ErrorKind::UnsupportedFormat,
            "Metadata can't be edited in an encrypted PDF.",
        ));
    }
    let mut notes = Vec::new();
    if edit.strip_all {
        strip_document_metadata(&mut doc);
        notes.push(
            "Title, author and all other document metadata (Info and XMP) were removed.".into(),
        );
    } else {
        let mut info = info_dict(&doc).cloned().unwrap_or_default();
        for (key, _, name) in FIELDS {
            if let Some(v) = edit.fields.get(key) {
                let v = v.trim();
                if v.is_empty() {
                    info.remove(name);
                } else {
                    info.set(name, encode_text(v));
                }
            }
        }
        let id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(id));
        let had_xmp = doc
            .catalog_mut()
            .map(|c| c.remove(b"Metadata").is_some())
            .unwrap_or(false);
        if had_xmp {
            notes.push(
                "The XMP copy of the metadata was removed so it can't contradict the new values."
                    .into(),
            );
        }
        notes.push("Page content was not changed.".into());
    }
    doc.prune_objects();
    let bytes = save_lopdf(&mut doc, false)?;
    Ok((bytes, notes))
}

/// Confirm the written file carries the requested values.
pub fn verify_metadata(data: &[u8], edit: &MetadataEdit) -> Result<()> {
    let fail = |m: String| RadialError::new(ErrorKind::EngineFailed, m);
    let doc = load_lopdf(data)?;
    let info = info_dict(&doc);
    if edit.strip_all {
        if info.is_some() || doc.catalog().is_ok_and(|c| c.has(b"Metadata")) {
            return Err(fail("Metadata is still present after removal.".into()));
        }
        return Ok(());
    }
    for (key, _, name) in FIELDS {
        if let Some(v) = edit.fields.get(key) {
            let got = info.and_then(|d| info_value(&doc, d, name));
            let want = (!v.trim().is_empty()).then(|| v.trim().to_string());
            if got != want {
                return Err(fail(format!("The {key} field was not saved correctly.")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        for s in ["Plain", "Café – €100", "مرحبا"] {
            let Object::String(b, _) = encode_text(s) else {
                panic!()
            };
            assert_eq!(decode_text(&b), s);
        }
        assert_eq!(pdf_date("D:20240131143000+01'00'"), "2024-01-31 14:30");
    }
}
