import { type PointerEvent, useEffect, useRef, type WheelEvent } from "react";
import { clampOverlay, overlayCornerRadius, overlayRect } from "../features/preview/overlay";
import type { CameraOverlay } from "../types/backend";

interface Props {
  screen: ImageData | null;
  camera: ImageData | null;
  /** Overlay settings, or null when the camera is off. */
  overlay: CameraOverlay | null;
  /** Aspect ratio of the source, used before the first frame arrives. */
  aspect: number;
  placeholder: string;
  /** Called while the user drags (or scroll-resizes) the camera overlay. */
  onOverlayChange?: (overlay: CameraOverlay) => void;
}

/** Internal canvas width; the element is scaled with CSS. */
const WIDTH = 960;

function toBitmap(image: ImageData): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  canvas.getContext("2d")?.putImageData(image, 0, 0);
  return canvas;
}

/**
 * Approximate preview of the final composition: the screen thumbnail with
 * the webcam where the recorder will put it.
 */
export function PreviewCanvas({
  screen,
  camera,
  overlay,
  aspect,
  placeholder,
  onOverlayChange,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const width = WIDTH;
  const height = Math.round(width / (screen ? screen.width / screen.height : aspect || 16 / 9));

  useEffect(() => {
    const ctx = canvasRef.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, width, height);
    ctx.fillStyle = "#0b0d14";
    ctx.fillRect(0, 0, width, height);
    if (screen) {
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(toBitmap(screen), 0, 0, width, height);
    }
    if (!overlay) return;

    const rect = overlayRect(width, height, overlay);
    const radius = overlayCornerRadius(rect, overlay);
    ctx.save();
    ctx.beginPath();
    ctx.roundRect(rect.x, rect.y, rect.width, rect.height, radius);
    ctx.clip();
    if (camera) {
      // Center-crop to the overlay's aspect ratio and mirror, like the recorder.
      const target = rect.width / rect.height;
      const source = camera.width / camera.height;
      const cropW = source > target ? camera.height * target : camera.width;
      const cropH = source > target ? camera.height : camera.width / target;
      ctx.translate(rect.x + rect.width, rect.y);
      ctx.scale(-1, 1);
      ctx.drawImage(
        toBitmap(camera),
        (camera.width - cropW) / 2,
        (camera.height - cropH) / 2,
        cropW,
        cropH,
        0,
        0,
        rect.width,
        rect.height,
      );
    } else {
      ctx.fillStyle = "rgba(120, 140, 255, 0.25)";
      ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
    }
    ctx.restore();
  }, [screen, camera, overlay, height]);

  /** Pointer position in canvas pixels (the canvas is letterboxed by CSS). */
  const toCanvas = (event: { clientX: number; clientY: number }) => {
    const element = canvasRef.current;
    if (!element) return null;
    const box = element.getBoundingClientRect();
    const scale = Math.min(box.width / width, box.height / height);
    const offsetX = (box.width - width * scale) / 2;
    const offsetY = (box.height - height * scale) / 2;
    return {
      x: (event.clientX - box.left - offsetX) / scale,
      y: (event.clientY - box.top - offsetY) / scale,
    };
  };

  const hitsOverlay = (point: { x: number; y: number }) => {
    if (!overlay) return false;
    const rect = overlayRect(width, height, overlay);
    return (
      point.x >= rect.x &&
      point.x <= rect.x + rect.width &&
      point.y >= rect.y &&
      point.y <= rect.y + rect.height
    );
  };

  const onPointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    const point = toCanvas(event);
    if (!overlay || !onOverlayChange || !point || !hitsOverlay(point)) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { dx: point.x - overlay.x * width, dy: point.y - overlay.y * height };
  };

  const onPointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    const point = toCanvas(event);
    if (!point) return;
    event.currentTarget.style.cursor = drag.current
      ? "grabbing"
      : onOverlayChange && hitsOverlay(point)
        ? "grab"
        : "default";
    if (!drag.current || !overlay || !onOverlayChange) return;
    const moved = {
      ...overlay,
      x: (point.x - drag.current.dx) / width,
      y: (point.y - drag.current.dy) / height,
    };
    onOverlayChange(clampOverlay(moved, width, height));
  };

  const onPointerUp = () => {
    drag.current = null;
  };

  const onWheel = (event: WheelEvent<HTMLCanvasElement>) => {
    const point = toCanvas(event);
    if (!overlay || !onOverlayChange || !point || !hitsOverlay(point)) return;
    const resized = { ...overlay, size: overlay.size * (event.deltaY < 0 ? 1.05 : 1 / 1.05) };
    onOverlayChange(clampOverlay(resized, width, height));
  };

  return (
    <div className="preview">
      <canvas
        ref={canvasRef}
        width={width}
        height={height}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onWheel={onWheel}
      />
      {!screen && (
        <div className="preview-placeholder" aria-hidden="true">
          {placeholder}
        </div>
      )}
    </div>
  );
}
