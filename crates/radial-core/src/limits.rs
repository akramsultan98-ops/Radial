//! Safety limits protecting against malformed or hostile inputs
//! (decompression bombs, absurd dimensions) and runaway memory use.

/// Largest source file RADIAL will open.
pub const MAX_SOURCE_BYTES: u64 = 1_000_000_000;
/// Largest width or height accepted when decoding or resizing.
pub const MAX_DIMENSION: u32 = 30_000;
/// Upper bound on a single decoder allocation.
pub const MAX_DECODE_ALLOC: u64 = 2 * 1024 * 1024 * 1024;
/// Largest edge of a generated preview.
pub const MAX_PREVIEW_DIMENSION: u32 = 8192;

pub fn decode_limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(MAX_DIMENSION);
    l.max_image_height = Some(MAX_DIMENSION);
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l
}
