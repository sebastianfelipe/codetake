// Mirrors `overlay_rect` in src-tauri/src/video/compositor.rs so the preview
// shows the webcam exactly where it will be in the recording.

import type { CameraOverlay } from "../../types/backend";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const HEIGHT_FRACTION = { small: 0.18, medium: 0.25, large: 0.34 } as const;

export function overlayRect(frameWidth: number, frameHeight: number, overlay: CameraOverlay): Rect {
  const shortSide = Math.min(frameWidth, frameHeight);
  const rawHeight = Math.round(frameHeight * HEIGHT_FRACTION[overlay.size]);
  const rawWidth = overlay.shape === "circle" ? rawHeight : Math.round((rawHeight * 4) / 3);
  const width = Math.max(2, Math.min(rawWidth, frameWidth));
  const height = Math.max(2, Math.min(rawHeight, frameHeight));
  const margin = Math.round(shortSide * 0.03);
  const x = overlay.position.endsWith("Left") ? margin : Math.max(0, frameWidth - width - margin);
  const y = overlay.position.startsWith("top")
    ? margin
    : Math.max(0, frameHeight - height - margin);
  return { x, y, width, height };
}

/** Corner radius used for the overlay shape (matches the compositor mask). */
export function overlayCornerRadius(rect: Rect, overlay: CameraOverlay): number {
  const short = Math.min(rect.width, rect.height);
  return overlay.shape === "circle" ? short / 2 : short * 0.16;
}
