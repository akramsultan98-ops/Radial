//! Process-wide application state.

use crate::access::Access;
use radial_core::detect::{self, FileInfo};
use radial_core::history::History;
use radial_core::settings::Settings;
use radial_core::{ErrorKind, RadialError, Registry};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Semaphore;

/// At most this many conversions run at once; the rest wait in a queue.
pub const MAX_CONCURRENT_JOBS: usize = 2;
/// At most this many dropped files are imported at once.
pub const MAX_IMPORT: usize = 50;

pub struct PreviewEntry {
    pub path: PathBuf,
    pub modified: Option<std::time::SystemTime>,
    pub max_dim: u32,
    pub page: u32,
    pub image: Arc<radial_core::image::DynamicImage>,
}

pub struct AppState {
    pub registry: Registry,
    pub access: Mutex<Access>,
    pub jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub slots: Semaphore,
    pub history: Mutex<History>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub previews: Mutex<Vec<PreviewEntry>>,
    launch_files: Mutex<Vec<PathBuf>>,
}

/// Lock a mutex, recovering from poisoning (a panicked job must not wedge the app).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(data_dir: &Path) -> Self {
        let settings_path = data_dir.join("settings.json");
        let settings = Settings::load(&settings_path);
        let mut access = Access::default();
        if let Some(dir) = &settings.default_destination {
            access.grant_dir(dir);
        }
        Self {
            registry: Registry::new(),
            access: Mutex::new(access),
            jobs: Mutex::new(HashMap::new()),
            slots: Semaphore::new(MAX_CONCURRENT_JOBS),
            history: Mutex::new(History::open(&data_dir.join("history.json"))),
            settings: Mutex::new(settings),
            settings_path,
            previews: Mutex::new(Vec::new()),
            launch_files: Mutex::new(Vec::new()),
        }
    }

    pub fn set_launch_files(&self, files: Vec<PathBuf>) {
        *lock(&self.launch_files) = files;
    }

    pub fn take_launch_files(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *lock(&self.launch_files))
    }

    /// Grant access to user-provided paths and inspect them. Folders and
    /// unreadable entries are reported rather than silently dropped.
    pub fn import(&self, paths: &[PathBuf]) -> Vec<ImportResult> {
        paths
            .iter()
            .take(MAX_IMPORT)
            .map(|p| match detect::inspect(p) {
                Ok(info) => {
                    lock(&self.access).grant_file(p);
                    ImportResult::File(Box::new(info))
                }
                Err(error) => ImportResult::Error {
                    name: p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    error,
                },
            })
            .collect()
    }

    pub fn ensure_file(&self, p: &Path) -> radial_core::Result<()> {
        lock(&self.access).check_file(p)
    }

    pub fn ensure_dir(&self, p: &Path) -> radial_core::Result<()> {
        lock(&self.access).check_dir(p)
    }

    /// Paths RADIAL may open or reveal: granted files and recorded outputs.
    pub fn ensure_openable(&self, p: &Path) -> radial_core::Result<()> {
        if self.ensure_file(p).is_ok() || lock(&self.history).has_output(p) {
            Ok(())
        } else {
            Err(RadialError::new(
                ErrorKind::PathNotAllowed,
                "RADIAL can only open files it created or you chose.",
            ))
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ImportResult {
    File(Box<FileInfo>),
    Error { name: String, error: RadialError },
}
