import { useEffect, useState } from "react";
import { api, events, type PreviewRequest, startPreview, subscribePreviewFrames } from "../lib/api";
import type { BackendError, PreviewKind } from "../types/backend";

export interface PreviewState {
  screen: ImageData | null;
  camera: ImageData | null;
  level: number;
  errors: Partial<Record<PreviewKind, BackendError>>;
}

const RESTART_DELAY_MS = 250;

/**
 * The live preview. While `enabled` (not recording) it runs a preview
 * session for the current selection, restarted (debounced) when the
 * selection changes. While recording, the recorder sends the thumbnails, so
 * the preview keeps moving instead of freezing on its last frame.
 */
export function usePreview(request: PreviewRequest, enabled: boolean): PreviewState {
  const [screen, setScreen] = useState<ImageData | null>(null);
  const [camera, setCamera] = useState<ImageData | null>(null);
  const [level, setLevel] = useState(0);
  const [errors, setErrors] = useState<PreviewState["errors"]>({});

  const key = JSON.stringify(request);

  // One channel for the lifetime of the window: thumbnails arrive from the
  // preview session or from the recorder.
  useEffect(() => {
    subscribePreviewFrames((frame) => {
      if (frame.kind === "screen") setScreen(frame.image);
      if (frame.kind === "camera") setCamera(frame.image);
    }).catch((error) => console.warn("preview unavailable", error));
  }, []);

  useEffect(() => {
    const unlisteners = [
      events.previewLevel(setLevel),
      events.previewError(({ kind, error }) => setErrors((e) => ({ ...e, [kind]: error }))),
    ];
    return () => {
      for (const unlisten of unlisteners) {
        void unlisten.then((fn) => fn());
      }
    };
  }, []);

  useEffect(() => {
    if (!enabled) {
      return;
    }
    const current: PreviewRequest = JSON.parse(key);
    setErrors({});
    setLevel(0);
    if (!current.source) setScreen(null);
    if (!current.cameraId) setCamera(null);

    const timer = window.setTimeout(() => {
      startPreview(current).catch((error) => console.warn("preview failed", error));
    }, RESTART_DELAY_MS);

    return () => window.clearTimeout(timer);
  }, [key, enabled]);

  // Release the devices when the preview is turned off or the app unmounts.
  useEffect(() => {
    if (!enabled) {
      void api.stopPreview();
      setLevel(0);
    }
    return () => {
      void api.stopPreview();
    };
  }, [enabled]);

  return { screen, camera, level, errors };
}
