//! RADIAL conversion core.
//!
//! Everything here is independent of the desktop shell so it can be tested
//! with real files and no GUI. See `docs/ARCHITECTURE.md`.

pub mod detect;
pub mod engine;
pub mod error;
pub mod format;
pub mod history;
pub mod limits;
pub mod naming;
pub mod options;
pub mod paths;
pub mod registry;
pub mod settings;
mod store;

pub use error::{ErrorKind, RadialError, Result};
pub use format::Format;
/// Re-exported so the shell uses the same image types.
pub use image;
pub use registry::Registry;
