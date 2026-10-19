//! Conversion options supplied by the UI, and their validation.

use crate::draw::{self, Shape};
use crate::error::{ErrorKind, RadialError, Result};
use crate::format::Format;
use crate::limits::MAX_DIMENSION;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConvertOptions {
    /// 1–100 for lossy encoders. `None` uses the format default.
    pub quality: Option<u8>,
    /// WebP only: encode losslessly (quality is then ignored).
    pub lossless: bool,
    /// `#rrggbb` used when flattening transparency. Defaults to white.
    pub background: Option<String>,
    pub resize: Option<Resize>,
    pub edits: Edits,
    /// Drawn into the pixels after resize (fractions of the edited image).
    pub annotations: Vec<Shape>,
    /// Areas replaced with opaque black after everything else.
    pub redactions: Vec<CropRect>,
    /// Border added around the image, in output pixels, filled with the background.
    pub padding: u32,
    /// Composite transparency onto the background even if the target keeps alpha.
    pub flatten: bool,
    /// Run lossless optimisation (PNG) — used by Compress.
    pub optimize: bool,
}

pub const MAX_PADDING: u32 = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resize {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// When both dimensions are given, fit inside the box instead of stretching.
    #[serde(default = "yes")]
    pub keep_aspect: bool,
}

fn yes() -> bool {
    true
}

/// Non-destructive edits, applied in this order: rotate → crop → resize →
/// brightness/contrast. The crop rectangle is expressed as fractions (0–1)
/// of the rotated image, so it is independent of preview resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Edits {
    /// Clockwise degrees: 0, 90, 180 or 270.
    pub rotate: u16,
    pub crop: Option<CropRect>,
    /// −100 … 100
    pub brightness: i8,
    /// −100 … 100
    pub contrast: i8,
}

impl Edits {
    pub fn is_identity(&self) -> bool {
        self.rotate == 0 && self.crop.is_none() && self.brightness == 0 && self.contrast == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub const WHITE: Rgb = Rgb([255, 255, 255]);

    pub fn parse(s: &str) -> Option<Rgb> {
        let hex = s.trim().strip_prefix('#')?;
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Rgb([byte(0)?, byte(2)?, byte(4)?]))
    }
}

pub const DEFAULT_JPEG_QUALITY: u8 = 90;
pub const DEFAULT_WEBP_QUALITY: u8 = 85;

/// Options after validation, with defaults resolved for a specific target.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub quality: u8,
    pub lossless: bool,
    pub background: Rgb,
    pub resize: Option<Resize>,
    pub edits: Edits,
    pub annotations: Vec<Shape>,
    pub redactions: Vec<CropRect>,
    pub padding: u32,
    pub flatten: bool,
    pub optimize: bool,
}

impl ConvertOptions {
    pub fn validate(&self, target: Format) -> Result<Resolved> {
        let bad = |m: &str| Err(RadialError::new(ErrorKind::InvalidOptions, m));

        let default_q = if target == Format::Webp {
            DEFAULT_WEBP_QUALITY
        } else {
            DEFAULT_JPEG_QUALITY
        };
        let quality = match self.quality {
            Some(q) if !(1..=100).contains(&q) => return bad("Quality must be between 1 and 100."),
            Some(_) if !target.has_quality() => {
                return bad(&format!(
                    "{} does not have a quality setting.",
                    target.label()
                ))
            }
            Some(q) => q,
            None => default_q,
        };
        if self.lossless && target != Format::Webp {
            return bad("Lossless mode is only available for WebP.");
        }
        let background = match &self.background {
            None => Rgb::WHITE,
            Some(s) => match Rgb::parse(s) {
                Some(c) => c,
                None => return bad("Background colour must look like #rrggbb."),
            },
        };
        if let Some(r) = self.resize {
            if r.width.is_none() && r.height.is_none() {
                return bad("Resize needs a width, a height, or both.");
            }
            for d in [r.width, r.height].into_iter().flatten() {
                if d == 0 || d > MAX_DIMENSION {
                    return bad(&format!(
                        "Dimensions must be between 1 and {MAX_DIMENSION} pixels."
                    ));
                }
            }
        }
        let e = self.edits;
        if !matches!(e.rotate, 0 | 90 | 180 | 270) {
            return bad("Rotation must be 0, 90, 180 or 270 degrees.");
        }
        if !(-100..=100).contains(&e.brightness) || !(-100..=100).contains(&e.contrast) {
            return bad("Brightness and contrast must be between −100 and 100.");
        }
        if let Some(c) = e.crop {
            let finite = [c.x, c.y, c.width, c.height].iter().all(|v| v.is_finite());
            let eps = 1e-9;
            if !finite
                || c.x < 0.0
                || c.y < 0.0
                || c.width <= 0.0
                || c.height <= 0.0
                || c.x + c.width > 1.0 + eps
                || c.y + c.height > 1.0 + eps
            {
                return bad("The crop area must lie inside the image.");
            }
        }
        draw::validate_shapes(&self.annotations)?;
        draw::validate_regions(&self.redactions)?;
        if self.padding > MAX_PADDING {
            return bad("Padding is too large.");
        }
        Ok(Resolved {
            quality,
            lossless: self.lossless,
            background,
            resize: self.resize,
            edits: e,
            annotations: self.annotations.clone(),
            redactions: self.redactions.clone(),
            padding: self.padding,
            flatten: self.flatten,
            optimize: self.optimize,
        })
    }
}

/// Final output size for `resize` applied to a `w`×`h` image.
pub fn resized_dimensions(w: u32, h: u32, r: &Resize) -> (u32, u32) {
    let scale = |num: u32, den: u32, v: u32| -> u32 {
        ((u64::from(v) * u64::from(num) + u64::from(den) / 2) / u64::from(den))
            .clamp(1, u64::from(MAX_DIMENSION)) as u32
    };
    match (r.width, r.height) {
        (Some(tw), None) => (tw, scale(tw, w, h)),
        (None, Some(th)) => (scale(th, h, w), th),
        (Some(tw), Some(th)) if !r.keep_aspect => (tw, th),
        (Some(tw), Some(th)) => {
            // Fit inside the box, preserving aspect ratio.
            if u64::from(tw) * u64::from(h) <= u64::from(th) * u64::from(w) {
                (tw, scale(tw, w, h))
            } else {
                (scale(th, h, w), th)
            }
        }
        (None, None) => (w, h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve_per_target() {
        let r = ConvertOptions::default().validate(Format::Jpeg).unwrap();
        assert_eq!(r.quality, DEFAULT_JPEG_QUALITY);
        assert_eq!(r.background, Rgb::WHITE);
        let r = ConvertOptions::default().validate(Format::Webp).unwrap();
        assert_eq!(r.quality, DEFAULT_WEBP_QUALITY);
    }

    #[test]
    fn rejects_out_of_range_values() {
        let o = |f: fn(&mut ConvertOptions)| {
            let mut o = ConvertOptions::default();
            f(&mut o);
            o
        };
        assert!(o(|o| o.quality = Some(0)).validate(Format::Jpeg).is_err());
        assert!(o(|o| o.quality = Some(101)).validate(Format::Jpeg).is_err());
        assert!(o(|o| o.quality = Some(80)).validate(Format::Png).is_err());
        assert!(o(|o| o.lossless = true).validate(Format::Jpeg).is_err());
        assert!(o(|o| o.background = Some("red".into()))
            .validate(Format::Jpeg)
            .is_err());
        assert!(o(|o| o.edits.rotate = 45).validate(Format::Png).is_err());
        assert!(o(|o| o.edits.brightness = 101)
            .validate(Format::Png)
            .is_err());
        assert!(o(|o| o.resize = Some(Resize {
            width: Some(0),
            height: None,
            keep_aspect: true
        }))
        .validate(Format::Png)
        .is_err());
        assert!(o(|o| o.resize = Some(Resize {
            width: None,
            height: None,
            keep_aspect: true
        }))
        .validate(Format::Png)
        .is_err());
        assert!(o(|o| o.edits.crop = Some(CropRect {
            x: 0.5,
            y: 0.0,
            width: 0.6,
            height: 1.0
        }))
        .validate(Format::Png)
        .is_err());
        assert!(o(|o| o.edits.crop = Some(CropRect {
            x: f64::NAN,
            y: 0.0,
            width: 0.5,
            height: 0.5
        }))
        .validate(Format::Png)
        .is_err());
    }

    #[test]
    fn accepts_valid_values() {
        let o = ConvertOptions {
            quality: Some(75),
            lossless: false,
            background: Some("#1a2B3c".into()),
            resize: Some(Resize {
                width: Some(100),
                height: None,
                keep_aspect: true,
            }),
            edits: Edits {
                rotate: 90,
                crop: Some(CropRect {
                    x: 0.1,
                    y: 0.1,
                    width: 0.9,
                    height: 0.9,
                }),
                brightness: -20,
                contrast: 30,
            },
            ..Default::default()
        };
        let r = o.validate(Format::Jpeg).unwrap();
        assert_eq!(r.background, Rgb([0x1a, 0x2b, 0x3c]));
    }

    #[test]
    fn computes_resize_dimensions() {
        let r = |w, h, k| Resize {
            width: w,
            height: h,
            keep_aspect: k,
        };
        assert_eq!(
            resized_dimensions(4000, 3000, &r(Some(800), None, true)),
            (800, 600)
        );
        assert_eq!(
            resized_dimensions(4000, 3000, &r(None, Some(300), true)),
            (400, 300)
        );
        assert_eq!(
            resized_dimensions(4000, 3000, &r(Some(800), Some(800), true)),
            (800, 600)
        );
        assert_eq!(
            resized_dimensions(3000, 4000, &r(Some(800), Some(800), true)),
            (600, 800)
        );
        assert_eq!(
            resized_dimensions(4000, 3000, &r(Some(800), Some(800), false)),
            (800, 800)
        );
        assert_eq!(resized_dimensions(10, 1, &r(Some(1), None, true)), (1, 1));
    }
}
