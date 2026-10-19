//! Local conversion history. Stores metadata only — never file contents.

use crate::error::{ErrorKind, Result};
use crate::format::Format;
use crate::ops::OperationId;
use crate::options::ConvertOptions;
use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub source_name: String,
    /// Kept so a failed conversion can be retried.
    pub source_path: PathBuf,
    pub source_format: Format,
    /// Older entries (before operations existed) were all conversions.
    #[serde(default)]
    pub operation: OperationId,
    /// Format of the result.
    pub target_format: Format,
    /// Unix epoch milliseconds.
    pub timestamp: u64,
    pub status: Status,
    pub output_path: Option<PathBuf>,
    pub error_kind: Option<ErrorKind>,
    pub error_message: Option<String>,
    pub dest_dir: PathBuf,
    pub stem: String,
    #[serde(default)]
    pub options: ConvertOptions,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    version: u32,
    entries: Vec<Entry>,
}

/// History persisted to a JSON file, newest first.
pub struct History {
    path: PathBuf,
    entries: Vec<Entry>,
}

impl History {
    pub fn open(path: &Path) -> Self {
        let f: File = store::load_or_default(path);
        Self {
            path: path.to_path_buf(),
            entries: f.entries,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn push(&mut self, entry: Entry) -> Result<()> {
        self.entries.retain(|e| e.id != entry.id);
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_ENTRIES);
        self.save()
    }

    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save()
    }

    /// Whether `path` is an output RADIAL recorded (used to authorise open/reveal).
    pub fn has_output(&self, path: &Path) -> bool {
        self.entries
            .iter()
            .any(|e| e.output_path.as_deref() == Some(path))
    }

    fn save(&self) -> Result<()> {
        store::save(
            &self.path,
            &File {
                version: 1,
                entries: self.entries.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.into(),
            source_name: "a.png".into(),
            source_path: "/x/a.png".into(),
            source_format: Format::Png,
            operation: OperationId::Convert,
            target_format: Format::Jpeg,
            timestamp: 1,
            status: Status::Failed,
            output_path: None,
            error_kind: Some(ErrorKind::DestinationExists),
            error_message: Some("exists".into()),
            dest_dir: "/x".into(),
            stem: "a".into(),
            options: ConvertOptions::default(),
        }
    }

    #[test]
    fn persists_round_trip_and_caps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/history.json");
        let mut h = History::open(&path);
        for i in 0..(MAX_ENTRIES + 5) {
            h.push(entry(&i.to_string())).unwrap();
        }
        let h2 = History::open(&path);
        assert_eq!(h2.entries().len(), MAX_ENTRIES);
        assert_eq!(h2.entries()[0].id, (MAX_ENTRIES + 4).to_string());
        assert_eq!(
            h2.entries()[0].error_kind,
            Some(ErrorKind::DestinationExists)
        );
    }

    #[test]
    fn corrupt_file_yields_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(History::open(&path).entries().is_empty());
    }

    #[test]
    fn clear_empties_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut h = History::open(&path);
        h.push(entry("1")).unwrap();
        h.clear().unwrap();
        assert!(History::open(&path).entries().is_empty());
    }
}
