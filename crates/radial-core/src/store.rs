//! Tiny atomic JSON persistence used by history and settings.

use crate::error::{RadialError, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::Path;

/// Read JSON from `path`, returning `T::default()` when the file is missing or
/// unreadable (a corrupt history must never stop the app from starting).
pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Write JSON atomically: temp file in the same directory, then rename.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)
        .map_err(|e| RadialError::from_io(&e, "creating the data folder"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| RadialError::from_io(&e, "saving data"))?;
    let json = serde_json::to_vec_pretty(value).expect("serialisable");
    tmp.write_all(&json)
        .map_err(|e| RadialError::from_io(&e, "saving data"))?;
    tmp.persist(path)
        .map_err(|e| RadialError::from_io(&e.error, "saving data"))?;
    Ok(())
}
