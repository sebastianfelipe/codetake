// Mirrors the overlay geometry in src-tauri/src/video/compositor.rs so the
// preview shows the webcam exactly where it will be in the recording.

import type { CameraOverlay } from "../../types/backend";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Limits of `CameraOverlay.size` (see `OVERLAY_MIN_SIZE` in config.rs). */
export const OVERLAY_MIN_SIZE = 0.08;
export const OVERLAY_MAX_SIZE = 0.6;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function overlaySize(frameWidth: number, frameHeight: number, overlay: CameraOverlay) {
  const rawHeight = Math.round(frameHeight * overlay.size);
  const rawWidth = overlay.shape === "rectangle" ? Math.round((rawHeight * 4) / 3) : rawHeight;
  return {
    width: clamp(rawWidth, 2, Math.max(2, frameWidth)),
    height: clamp(rawHeight, 2, Math.max(2, frameHeight)),
  };
}

export function overlayRect(frameWidth: number, frameHeight: number, overlay: CameraOverlay): Rect {
  const { width, height } = overlaySize(frameWidth, frameHeight, overlay);
  const place = (center: number, extent: number, frame: number) =>
    clamp(Math.round(center * frame - extent / 2), 0, Math.max(0, frame - extent));
  return {
    x: place(overlay.x, width, frameWidth),
    y: place(overlay.y, height, frameHeight),
    width,
    height,
  };
}

export function overlayCornerRadius(rect: Rect, overlay: CameraOverlay): number {
  const short = Math.min(rect.width, rect.height);
  return overlay.shape === "circle" ? short / 2 : short * 0.12;
}

/**
 * Keeps the overlay's center where the whole overlay stays inside the frame,
 * so a dragged overlay never ends up partly off screen.
 */
export function clampOverlay(
  overlay: CameraOverlay,
  frameWidth: number,
  frameHeight: number,
): CameraOverlay {
  const size = clamp(overlay.size, OVERLAY_MIN_SIZE, OVERLAY_MAX_SIZE);
  const { width, height } = overlaySize(frameWidth, frameHeight, { ...overlay, size });
  const halfW = width / 2 / frameWidth;
  const halfH = height / 2 / frameHeight;
  return {
    ...overlay,
    size,
    x: clamp(overlay.x, halfW, 1 - halfW),
    y: clamp(overlay.y, halfH, 1 - halfH),
  };
}

/** Quick placements offered next to free dragging. */
export const OVERLAY_PRESETS = {
  topLeft: { x: 0.09, y: 0.16 },
  topRight: { x: 0.91, y: 0.16 },
  bottomLeft: { x: 0.09, y: 0.84 },
  bottomRight: { x: 0.91, y: 0.84 },
} as const;
