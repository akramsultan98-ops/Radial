//! User preferences, persisted locally.

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotionPreference {
    #[default]
    System,
    Reduced,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `None` means "next to the source file".
    pub default_destination: Option<PathBuf>,
    pub jpeg_quality: u8,
    pub webp_quality: u8,
    /// `#rrggbb` used when flattening transparency.
    pub background: String,
    pub keep_history: bool,
    pub motion: MotionPreference,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_destination: None,
            jpeg_quality: crate::options::DEFAULT_JPEG_QUALITY,
            webp_quality: crate::options::DEFAULT_WEBP_QUALITY,
            background: "#ffffff".into(),
            keep_history: true,
            motion: MotionPreference::System,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s: Settings = store::load_or_default(path);
        s.jpeg_quality = s.jpeg_quality.clamp(1, 100);
        s.webp_quality = s.webp_quality.clamp(1, 100);
        if crate::options::Rgb::parse(&s.background).is_none() {
            s.background = Settings::default().background;
        }
        s
    }

    pub fn save(&self, path: &Path) -> crate::error::Result<()> {
        store::save(path, self)
    }
}
