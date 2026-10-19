//! Documents: Word (.docx) text and properties, plain text → PDF.
//!
//! Only conversions that can be done faithfully are offered. DOCX → PDF
//! needs a layout engine (LibreOffice, Word) and is not offered; DOCX → text
//! keeps words and paragraphs but no formatting.

pub mod docx;
pub mod text;
