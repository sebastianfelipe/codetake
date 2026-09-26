import { useEffect, useRef } from "react";
import { overlayCornerRadius, overlayRect } from "../features/preview/overlay";
import type { CameraOverlay } from "../types/backend";

interface Props {
  screen: ImageData | null;
  camera: ImageData | null;
  /** Overlay settings, or null when the camera is off. */
  overlay: CameraOverlay | null;
  /** Aspect ratio of the source, used before the first frame arrives. */
  aspect: number;
  placeholder: string;
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
export function PreviewCanvas({ screen, camera, overlay, aspect, placeholder }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
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

  return (
    <div className="preview">
      <canvas ref={canvasRef} width={width} height={height} />
      {!screen && <div className="preview-placeholder">{placeholder}</div>}
    </div>
  );
}
