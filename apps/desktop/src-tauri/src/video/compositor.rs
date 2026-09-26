//! Composites the webcam overlay onto screen frames.
//!
//! Frames are 8-bit BGRA, the native format of ScreenCaptureKit and
//! AVFoundation on macOS. The camera image is center-cropped to the overlay's
//! aspect ratio, scaled with bilinear filtering and blended through an
//! anti-aliased shape mask (circle or rounded rectangle).

use crate::config::{CameraOverlay, OverlayPosition, OverlayShape};

/// A read-only view of a BGRA image.
#[derive(Clone, Copy)]
pub struct BgraRef<'a> {
    pub data: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

/// A mutable view of a BGRA image.
pub struct BgraMut<'a> {
    pub data: &'a mut [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Where the camera overlay goes in a frame of the given size.
pub fn overlay_rect(frame_width: usize, frame_height: usize, overlay: &CameraOverlay) -> Rect {
    let short_side = frame_width.min(frame_height) as f64;
    let height = (frame_height as f64 * overlay.size.height_fraction()).round() as usize;
    let width = match overlay.shape {
        OverlayShape::Circle => height,
        OverlayShape::RoundedRectangle => (height as f64 * 4.0 / 3.0).round() as usize,
    };
    let width = width.min(frame_width).max(2);
    let height = height.min(frame_height).max(2);
    let margin = (short_side * 0.03).round() as usize;

    let left = margin;
    let right = frame_width.saturating_sub(width + margin);
    let top = margin;
    let bottom = frame_height.saturating_sub(height + margin);
    let (x, y) = match overlay.position {
        OverlayPosition::TopLeft => (left, top),
        OverlayPosition::TopRight => (right, top),
        OverlayPosition::BottomLeft => (left, bottom),
        OverlayPosition::BottomRight => (right, bottom),
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Per-pixel coverage (0..=255) of the overlay shape.
#[derive(Debug, Clone)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    coverage: Vec<u8>,
}

impl Mask {
    pub fn new(width: usize, height: usize, shape: OverlayShape) -> Self {
        let radius = match shape {
            OverlayShape::Circle => width.min(height) as f64 / 2.0,
            OverlayShape::RoundedRectangle => width.min(height) as f64 * 0.16,
        };
        let half_w = width as f64 / 2.0;
        let half_h = height as f64 / 2.0;
        let mut coverage = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                // Signed distance from the rounded-rectangle edge (negative inside).
                let px = (x as f64 + 0.5 - half_w).abs() - (half_w - radius);
                let py = (y as f64 + 0.5 - half_h).abs() - (half_h - radius);
                let outside = px.max(0.0).hypot(py.max(0.0));
                let inside = px.max(py).min(0.0);
                let distance = outside + inside - radius;
                let alpha = (0.5 - distance).clamp(0.0, 1.0);
                coverage.push((alpha * 255.0).round() as u8);
            }
        }
        Self {
            width,
            height,
            coverage,
        }
    }

    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.coverage[y * self.width + x]
    }
}

/// Copies `src` into `dst`. Sizes must match; strides may differ.
pub fn copy_frame(src: BgraRef<'_>, dst: &mut BgraMut<'_>) {
    let rows = src.height.min(dst.height);
    let row_bytes = src.width.min(dst.width) * 4;
    for y in 0..rows {
        let s = &src.data[y * src.stride..y * src.stride + row_bytes];
        dst.data[y * dst.stride..y * dst.stride + row_bytes].copy_from_slice(s);
    }
}

/// Scales `src` to fill `dst` (nearest neighbour). Used only as a fallback
/// when a captured frame's size does not match the output size, e.g. for a
/// few frames while a captured window is being resized.
pub fn scale_frame_nearest(src: BgraRef<'_>, dst: &mut BgraMut<'_>) {
    if src.width == 0 || src.height == 0 {
        return;
    }
    for y in 0..dst.height {
        let sy = y * src.height / dst.height;
        let src_row = &src.data[sy * src.stride..];
        let dst_row = &mut dst.data[y * dst.stride..];
        for x in 0..dst.width {
            let sx = x * src.width / dst.width;
            dst_row[x * 4..x * 4 + 4].copy_from_slice(&src_row[sx * 4..sx * 4 + 4]);
        }
    }
}

/// Fills `dst` with opaque black.
pub fn clear_frame(dst: &mut BgraMut<'_>) {
    for y in 0..dst.height {
        let row = &mut dst.data[y * dst.stride..y * dst.stride + dst.width * 4];
        for pixel in row.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

/// The part of the camera image used for an overlay of the given aspect
/// ratio (a centered crop), in source pixel coordinates.
fn center_crop(src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> (f64, f64, f64, f64) {
    let src_aspect = src_w as f64 / src_h as f64;
    let dst_aspect = dst_w as f64 / dst_h as f64;
    if src_aspect > dst_aspect {
        let crop_w = src_h as f64 * dst_aspect;
        ((src_w as f64 - crop_w) / 2.0, 0.0, crop_w, src_h as f64)
    } else {
        let crop_h = src_w as f64 / dst_aspect;
        (0.0, (src_h as f64 - crop_h) / 2.0, src_w as f64, crop_h)
    }
}

fn sample_bilinear(src: &BgraRef<'_>, x: f64, y: f64) -> [f32; 3] {
    let x = x.clamp(0.0, (src.width - 1) as f64);
    let y = y.clamp(0.0, (src.height - 1) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(src.width - 1);
    let y1 = (y0 + 1).min(src.height - 1);
    let tx = (x - x0 as f64) as f32;
    let ty = (y - y0 as f64) as f32;
    let px = |xx: usize, yy: usize, c: usize| f32::from(src.data[yy * src.stride + xx * 4 + c]);
    let mut out = [0.0; 3];
    for (c, value) in out.iter_mut().enumerate() {
        let top = px(x0, y0, c) + (px(x1, y0, c) - px(x0, y0, c)) * tx;
        let bottom = px(x0, y1, c) + (px(x1, y1, c) - px(x0, y1, c)) * tx;
        *value = top + (bottom - top) * ty;
    }
    out
}

/// Draws `camera` into `dst` at `rect`, cropped, scaled and masked.
///
/// When `mirror` is set the camera is flipped horizontally, matching what
/// people expect from a selfie view.
pub fn blend_overlay(
    dst: &mut BgraMut<'_>,
    camera: BgraRef<'_>,
    rect: Rect,
    mask: &Mask,
    mirror: bool,
) {
    if camera.width < 2 || camera.height < 2 || mask.width != rect.width {
        return;
    }
    let (crop_x, crop_y, crop_w, crop_h) =
        center_crop(camera.width, camera.height, rect.width, rect.height);
    let scale_x = crop_w / rect.width as f64;
    let scale_y = crop_h / rect.height as f64;

    let rows = rect.height.min(dst.height.saturating_sub(rect.y));
    let cols = rect.width.min(dst.width.saturating_sub(rect.x));
    for y in 0..rows {
        let sy = crop_y + (y as f64 + 0.5) * scale_y - 0.5;
        let row_start = (rect.y + y) * dst.stride + rect.x * 4;
        for x in 0..cols {
            let alpha = mask.at(x, y);
            if alpha == 0 {
                continue;
            }
            let mx = if mirror { rect.width - 1 - x } else { x };
            let sx = crop_x + (mx as f64 + 0.5) * scale_x - 0.5;
            let color = sample_bilinear(&camera, sx, sy);
            let pixel = &mut dst.data[row_start + x * 4..row_start + x * 4 + 4];
            let a = f32::from(alpha) / 255.0;
            for c in 0..3 {
                let blended = color[c] * a + f32::from(pixel[c]) * (1.0 - a);
                pixel[c] = blended.round() as u8;
            }
            pixel[3] = 255;
        }
    }
}

/// Downscales a BGRA frame to fit within `max_width` × `max_height`,
/// returning tightly packed RGBA (ready for an HTML canvas).
pub fn thumbnail_rgba(
    src: BgraRef<'_>,
    max_width: usize,
    max_height: usize,
) -> (usize, usize, Vec<u8>) {
    if src.width == 0 || src.height == 0 {
        return (0, 0, Vec::new());
    }
    let scale = (max_width as f64 / src.width as f64)
        .min(max_height as f64 / src.height as f64)
        .min(1.0);
    let width = ((src.width as f64 * scale).round() as usize).max(1);
    let height = ((src.height as f64 * scale).round() as usize).max(1);
    let mut out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let sy = (y * src.height / height).min(src.height - 1);
        for x in 0..width {
            let sx = (x * src.width / width).min(src.width - 1);
            let i = sy * src.stride + sx * 4;
            out.extend_from_slice(&[src.data[i + 2], src.data[i + 1], src.data[i], 255]);
        }
    }
    (width, height, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OverlaySize;

    fn solid(width: usize, height: usize, bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat(width * height)
    }

    fn overlay(position: OverlayPosition, shape: OverlayShape) -> CameraOverlay {
        CameraOverlay {
            size: OverlaySize::Medium,
            position,
            shape,
        }
    }

    #[test]
    fn circle_overlay_is_square_in_the_bottom_right_by_default() {
        let rect = overlay_rect(1920, 1080, &CameraOverlay::default());
        assert_eq!(rect.width, 270);
        assert_eq!(rect.height, 270);
        let margin = (1080.0_f64 * 0.03).round() as usize;
        assert_eq!(rect.x + rect.width + margin, 1920);
        assert_eq!(rect.y + rect.height + margin, 1080);
    }

    #[test]
    fn overlay_positions_map_to_corners() {
        let top_left = overlay_rect(
            1000,
            500,
            &overlay(OverlayPosition::TopLeft, OverlayShape::Circle),
        );
        assert_eq!((top_left.x, top_left.y), (15, 15));
        let top_right = overlay_rect(
            1000,
            500,
            &overlay(OverlayPosition::TopRight, OverlayShape::Circle),
        );
        assert_eq!(top_right.x + top_right.width, 985);
        let bottom_left = overlay_rect(
            1000,
            500,
            &overlay(OverlayPosition::BottomLeft, OverlayShape::Circle),
        );
        assert_eq!(bottom_left.y + bottom_left.height, 485);
    }

    #[test]
    fn rounded_rectangle_overlay_is_landscape() {
        let rect = overlay_rect(
            1920,
            1080,
            &overlay(OverlayPosition::BottomRight, OverlayShape::RoundedRectangle),
        );
        assert!(rect.width > rect.height);
    }

    #[test]
    fn overlay_fits_inside_tiny_frames() {
        let rect = overlay_rect(40, 20, &CameraOverlay::default());
        assert!(rect.x + rect.width <= 40 && rect.y + rect.height <= 20);
    }

    #[test]
    fn circle_mask_is_opaque_in_the_center_and_clear_in_the_corners() {
        let mask = Mask::new(100, 100, OverlayShape::Circle);
        assert_eq!(mask.at(50, 50), 255);
        assert_eq!(mask.at(0, 0), 0);
        assert_eq!(mask.at(99, 99), 0);
        // The edge is anti-aliased.
        let edge = mask.at(50, 0);
        assert!(edge > 0);
    }

    #[test]
    fn rounded_rectangle_mask_covers_edges_but_not_corners() {
        let mask = Mask::new(120, 90, OverlayShape::RoundedRectangle);
        assert_eq!(mask.at(60, 1), 255);
        assert_eq!(mask.at(1, 45), 255);
        assert_eq!(mask.at(0, 0), 0);
    }

    #[test]
    fn copies_between_different_strides() {
        let src = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0,
        ];
        let mut out = vec![0; 16];
        let mut dst = BgraMut {
            data: &mut out,
            width: 2,
            height: 2,
            stride: 8,
        };
        copy_frame(
            BgraRef {
                data: &src,
                width: 2,
                height: 2,
                stride: 10,
            },
            &mut dst,
        );
        assert_eq!(
            out,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn blends_camera_only_inside_the_mask() {
        let (w, h) = (200, 100);
        let mut frame = solid(w, h, [0, 0, 0, 255]);
        let camera = solid(64, 48, [10, 200, 30, 255]);
        let config = CameraOverlay::default();
        let rect = overlay_rect(w, h, &config);
        let mask = Mask::new(rect.width, rect.height, config.shape);
        let mut dst = BgraMut {
            data: &mut frame,
            width: w,
            height: h,
            stride: w * 4,
        };
        blend_overlay(
            &mut dst,
            BgraRef {
                data: &camera,
                width: 64,
                height: 48,
                stride: 64 * 4,
            },
            rect,
            &mask,
            true,
        );
        let at = |x: usize, y: usize| &frame[(y * w + x) * 4..(y * w + x) * 4 + 4];
        let cx = rect.x + rect.width / 2;
        let cy = rect.y + rect.height / 2;
        assert_eq!(at(cx, cy), &[10, 200, 30, 255]);
        assert_eq!(at(rect.x, rect.y), &[0, 0, 0, 255]);
        assert_eq!(at(0, 0), &[0, 0, 0, 255]);
    }

    #[test]
    fn center_crop_keeps_the_middle_of_wide_cameras() {
        let (x, y, w, h) = center_crop(1280, 720, 100, 100);
        assert_eq!((x, y, w, h), (280.0, 0.0, 720.0, 720.0));
    }

    #[test]
    fn nearest_scaling_fills_the_destination() {
        let src = solid(2, 2, [1, 2, 3, 255]);
        let mut out = vec![0; 4 * 4 * 4];
        let mut dst = BgraMut {
            data: &mut out,
            width: 4,
            height: 4,
            stride: 16,
        };
        scale_frame_nearest(
            BgraRef {
                data: &src,
                width: 2,
                height: 2,
                stride: 8,
            },
            &mut dst,
        );
        assert!(out.chunks(4).all(|p| p == [1, 2, 3, 255]));
    }

    #[test]
    fn thumbnails_are_rgba_and_fit_the_bounds() {
        let src = solid(1920, 1080, [10, 20, 30, 255]);
        let (w, h, rgba) = thumbnail_rgba(
            BgraRef {
                data: &src,
                width: 1920,
                height: 1080,
                stride: 1920 * 4,
            },
            640,
            640,
        );
        assert_eq!((w, h), (640, 360));
        assert_eq!(rgba.len(), 640 * 360 * 4);
        assert_eq!(&rgba[..4], &[30, 20, 10, 255]);
    }
}
