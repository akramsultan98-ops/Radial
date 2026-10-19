//! Annotations and redaction boxes drawn into image pixels.
//!
//! Geometry is expressed in fractions (0–1) of the edited image (after rotate
//! and crop, before resize), so what the user draws on the preview lands in
//! the same place at any output size. Stroke widths and text sizes are
//! fractions of the image's shorter side.
//!
//! Everything here modifies pixels: a redaction box replaces the pixels
//! underneath, it is not an overlay that can be removed later.

use crate::error::{ErrorKind, RadialError, Result};
use crate::options::{CropRect, Rgb};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_circle_mut, draw_filled_ellipse_mut, draw_filled_rect_mut, draw_polygon_mut,
    draw_text_mut,
};
use imageproc::point::Point;
use imageproc::rect::Rect;
use serde::{Deserialize, Serialize};

/// Bundled so annotation text renders identically everywhere (Bitstream Vera
/// licence, see assets/fonts/DejaVu-LICENSE.txt).
pub static FONT_DATA: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

pub const MAX_SHAPES: usize = 500;
pub const MAX_TEXT_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
    Arrow,
    Text,
}

/// One annotation. Rectangles and ellipses span (x1,y1)–(x2,y2); lines and
/// arrows run from (x1,y1) to (x2,y2); text starts at (x1,y1) (top-left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub kind: ShapeKind,
    pub x1: f64,
    pub y1: f64,
    #[serde(default)]
    pub x2: f64,
    #[serde(default)]
    pub y2: f64,
    /// `#rrggbb`
    pub color: String,
    /// Stroke width as a fraction of the shorter side.
    #[serde(default = "default_stroke")]
    pub stroke: f64,
    #[serde(default)]
    pub filled: bool,
    #[serde(default)]
    pub text: String,
    /// Text height as a fraction of the shorter side.
    #[serde(default = "default_size")]
    pub size: f64,
    /// PDF page (0-based) the shape belongs to; ignored for images.
    #[serde(default)]
    pub page: u32,
}

fn default_stroke() -> f64 {
    0.006
}
fn default_size() -> f64 {
    0.05
}

fn bad(msg: &str) -> RadialError {
    RadialError::new(ErrorKind::InvalidOptions, msg)
}

/// Validate annotations; returns the parsed colours in order.
pub fn validate_shapes(shapes: &[Shape]) -> Result<()> {
    if shapes.len() > MAX_SHAPES {
        return Err(bad("Too many annotations (the limit is 500)."));
    }
    for s in shapes {
        let coords = [s.x1, s.y1, s.x2, s.y2, s.stroke, s.size];
        if !coords.iter().all(|v| v.is_finite()) {
            return Err(bad("An annotation has an invalid position."));
        }
        let within = |v: f64| (-0.01..=1.01).contains(&v);
        if !within(s.x1) || !within(s.y1) || !within(s.x2) || !within(s.y2) {
            return Err(bad("Annotations must lie inside the image."));
        }
        if !(0.0005..=0.2).contains(&s.stroke) {
            return Err(bad("Annotation line width is out of range."));
        }
        if Rgb::parse(&s.color).is_none() {
            return Err(bad("Annotation colours must look like #rrggbb."));
        }
        if s.kind == ShapeKind::Text {
            if s.text.trim().is_empty() {
                return Err(bad("A text annotation is empty."));
            }
            if s.text.chars().count() > MAX_TEXT_CHARS {
                return Err(bad("A text annotation is too long."));
            }
            if !(0.005..=0.5).contains(&s.size) {
                return Err(bad("Annotation text size is out of range."));
            }
        }
    }
    Ok(())
}

pub fn validate_regions(regions: &[CropRect]) -> Result<()> {
    if regions.len() > MAX_SHAPES {
        return Err(bad("Too many redaction areas."));
    }
    for r in regions {
        let ok = [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
            && r.x >= -1e-9
            && r.y >= -1e-9
            && r.width > 0.0
            && r.height > 0.0
            && r.x + r.width <= 1.0 + 1e-9
            && r.y + r.height <= 1.0 + 1e-9;
        if !ok {
            return Err(bad("Redaction areas must lie inside the image."));
        }
    }
    Ok(())
}

/// Draw annotations and redactions. Returns an 8-bit image (RGBA when the
/// input had alpha, RGB otherwise). Redactions are drawn last so nothing can
/// be drawn on top of them, and they are fully opaque.
pub fn render(img: DynamicImage, shapes: &[Shape], redactions: &[CropRect]) -> DynamicImage {
    if shapes.is_empty() && redactions.is_empty() {
        return img;
    }
    let had_alpha = img.color().has_alpha();
    let mut canvas = img.to_rgba8();
    let font = FontRef::try_from_slice(FONT_DATA).expect("bundled font is valid");
    for s in shapes {
        draw_shape(&mut canvas, s, &font);
    }
    for r in redactions {
        let (w, h) = (f64::from(canvas.width()), f64::from(canvas.height()));
        // Round outward so partially covered pixels are covered too.
        let x0 = (r.x * w).floor().max(0.0) as i32;
        let y0 = (r.y * h).floor().max(0.0) as i32;
        let x1 = ((r.x + r.width) * w).ceil().min(w) as i32;
        let y1 = ((r.y + r.height) * h).ceil().min(h) as i32;
        if x1 > x0 && y1 > y0 {
            let rect = Rect::at(x0, y0).of_size((x1 - x0) as u32, (y1 - y0) as u32);
            draw_filled_rect_mut(&mut canvas, rect, Rgba([0, 0, 0, 255]));
        }
    }
    if had_alpha {
        DynamicImage::ImageRgba8(canvas)
    } else {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(canvas).to_rgb8())
    }
}

fn draw_shape(canvas: &mut RgbaImage, s: &Shape, font: &FontRef<'_>) {
    let (w, h) = (f64::from(canvas.width()), f64::from(canvas.height()));
    let short = w.min(h);
    let Rgb([r, g, b]) = Rgb::parse(&s.color).unwrap_or(Rgb([255, 0, 0]));
    let color = Rgba([r, g, b, 255]);
    let px = |x: f64, y: f64| (x * w, y * h);
    let stroke = (s.stroke * short).max(1.0);
    let (ax, ay) = px(s.x1, s.y1);
    let (bx, by) = px(s.x2, s.y2);
    match s.kind {
        ShapeKind::Line => thick_line(canvas, (ax, ay), (bx, by), stroke, color),
        ShapeKind::Arrow => {
            let (dx, dy) = (bx - ax, by - ay);
            let len = dx.hypot(dy);
            if len < 1.0 {
                return;
            }
            let (ux, uy) = (dx / len, dy / len);
            let head = (stroke * 4.0).max(short * 0.02).min(len * 0.6);
            let base = (bx - ux * head, by - uy * head);
            thick_line(canvas, (ax, ay), base, stroke, color);
            let half = head * 0.55;
            let poly = [
                pt(bx, by),
                pt(base.0 - uy * half, base.1 + ux * half),
                pt(base.0 + uy * half, base.1 - ux * half),
            ];
            draw_polygon_safe(canvas, &poly, color);
        }
        ShapeKind::Rect => {
            let (x0, x1) = (ax.min(bx), ax.max(bx));
            let (y0, y1) = (ay.min(by), ay.max(by));
            if s.filled {
                let rect = Rect::at(x0.round() as i32, y0.round() as i32).of_size(
                    ((x1 - x0).round() as u32).max(1),
                    ((y1 - y0).round() as u32).max(1),
                );
                draw_filled_rect_mut(canvas, rect, color);
            } else {
                for (p, q) in [
                    ((x0, y0), (x1, y0)),
                    ((x1, y0), (x1, y1)),
                    ((x1, y1), (x0, y1)),
                    ((x0, y1), (x0, y0)),
                ] {
                    thick_line(canvas, p, q, stroke, color);
                }
            }
        }
        ShapeKind::Ellipse => {
            let (cx, cy) = ((ax + bx) / 2.0, (ay + by) / 2.0);
            let (rx, ry) = (((bx - ax) / 2.0).abs(), ((by - ay) / 2.0).abs());
            if s.filled {
                draw_filled_ellipse_mut(
                    canvas,
                    (cx as i32, cy as i32),
                    rx as i32,
                    ry as i32,
                    color,
                );
            } else {
                let n = 96;
                let points: Vec<(f64, f64)> = (0..=n)
                    .map(|i| {
                        let t = std::f64::consts::TAU * f64::from(i) / f64::from(n);
                        (cx + rx * t.cos(), cy + ry * t.sin())
                    })
                    .collect();
                for pair in points.windows(2) {
                    thick_line(canvas, pair[0], pair[1], stroke, color);
                }
            }
        }
        ShapeKind::Text => {
            let size = (s.size * short).max(6.0) as f32;
            let scale = PxScale::from(size);
            let line_height = font.as_scaled(scale).height() * 1.15;
            for (i, line) in s.text.lines().enumerate() {
                let y = ay as f32 + line_height * i as f32;
                draw_text_mut(canvas, color, ax as i32, y as i32, scale, font, line);
            }
        }
    }
}

fn pt(x: f64, y: f64) -> Point<i32> {
    Point::new(x.round() as i32, y.round() as i32)
}

/// imageproc rejects polygons whose first and last points coincide.
fn draw_polygon_safe(canvas: &mut RgbaImage, poly: &[Point<i32>], color: Rgba<u8>) {
    let mut pts: Vec<Point<i32>> = Vec::with_capacity(poly.len());
    for p in poly {
        if pts.last() != Some(p) {
            pts.push(*p);
        }
    }
    while pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() >= 3 {
        draw_polygon_mut(canvas, &pts, color);
    }
}

/// A line of the given width with round caps.
fn thick_line(canvas: &mut RgbaImage, a: (f64, f64), b: (f64, f64), width: f64, color: Rgba<u8>) {
    let r = width / 2.0;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if len >= 0.5 {
        let (nx, ny) = (-dy / len * r, dx / len * r);
        let poly = [
            pt(a.0 + nx, a.1 + ny),
            pt(b.0 + nx, b.1 + ny),
            pt(b.0 - nx, b.1 - ny),
            pt(a.0 - nx, a.1 - ny),
        ];
        draw_polygon_safe(canvas, &poly, color);
    }
    let rad = r.round().max(1.0) as i32;
    draw_filled_circle_mut(canvas, (a.0.round() as i32, a.1.round() as i32), rad, color);
    draw_filled_circle_mut(canvas, (b.0.round() as i32, b.1.round() as i32), rad, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            w,
            h,
            image::Rgb([255, 255, 255]),
        ))
    }

    fn shape(kind: ShapeKind) -> Shape {
        Shape {
            kind,
            x1: 0.1,
            y1: 0.1,
            x2: 0.9,
            y2: 0.9,
            color: "#ff0000".into(),
            stroke: 0.02,
            filled: false,
            text: String::new(),
            size: 0.1,
            page: 0,
        }
    }

    #[test]
    fn redaction_replaces_pixels_with_opaque_black() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 50, Rgba([10, 200, 30, 0])));
        let out = render(
            img,
            &[],
            &[CropRect {
                x: 0.2,
                y: 0.2,
                width: 0.5,
                height: 0.5,
            }],
        )
        .to_rgba8();
        assert_eq!(out.get_pixel(30, 15).0, [0, 0, 0, 255]);
        assert_eq!(out.get_pixel(5, 5).0, [10, 200, 30, 0], "outside untouched");
    }

    #[test]
    fn draws_each_shape_kind() {
        for kind in [
            ShapeKind::Rect,
            ShapeKind::Ellipse,
            ShapeKind::Line,
            ShapeKind::Arrow,
        ] {
            let out = render(white(200, 200), &[shape(kind)], &[]).to_rgb8();
            let red = out.pixels().filter(|p| p.0 == [255, 0, 0]).count();
            assert!(red > 100, "{kind:?} drew {red} pixels");
        }
        let mut t = shape(ShapeKind::Text);
        t.text = "Hello".into();
        let out = render(white(200, 200), &[t], &[]).to_rgb8();
        assert!(out.pixels().any(|p| p.0[1] < 128), "text drew nothing");
    }

    #[test]
    fn filled_rect_covers_area() {
        let mut s = shape(ShapeKind::Rect);
        s.filled = true;
        let out = render(white(100, 100), &[s], &[]).to_rgb8();
        assert_eq!(out.get_pixel(50, 50).0, [255, 0, 0]);
    }

    #[test]
    fn validation_rejects_bad_input() {
        let mut s = shape(ShapeKind::Line);
        s.color = "red".into();
        assert!(validate_shapes(&[s]).is_err());
        let mut s = shape(ShapeKind::Text);
        s.text = "  ".into();
        assert!(validate_shapes(&[s]).is_err());
        let mut s = shape(ShapeKind::Line);
        s.x2 = f64::NAN;
        assert!(validate_shapes(&[s]).is_err());
        assert!(validate_regions(&[CropRect {
            x: 0.8,
            y: 0.0,
            width: 0.5,
            height: 0.5
        }])
        .is_err());
        assert!(validate_shapes(&[shape(ShapeKind::Arrow)]).is_ok());
    }
}
