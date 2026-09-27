import { useCallback, useEffect, useState } from "react";
import { api, toBackendError } from "../lib/api";
import type {
  BackendError,
  CameraInfo,
  DisplayInfo,
  MicrophoneInfo,
  MusicTrack,
  PermissionState,
  PlatformCapabilities,
  WindowInfo,
} from "../types/backend";

export interface BackendData {
  capabilities: PlatformCapabilities;
  permissions: PermissionState;
  displays: DisplayInfo[];
  windows: WindowInfo[];
  cameras: CameraInfo[];
  microphones: MicrophoneInfo[];
  tracks: MusicTrack[];
  defaultOutputDirectory: string | null;
}

async function settle<T>(promise: Promise<T>, fallback: T, errors: BackendError[]): Promise<T> {
  try {
    return await promise;
  } catch (error) {
    errors.push(toBackendError(error));
    return fallback;
  }
}

async function loadAll(): Promise<{ data: BackendData; errors: BackendError[] }> {
  const errors: BackendError[] = [];
  const [capabilities, permissions] = await Promise.all([api.capabilities(), api.permissions()]);
  const screenAllowed =
    permissions.screenRecording === "granted" || permissions.screenRecording === "notRequired";
  // Listing screens needs Screen Recording permission; don't report that as an error.
  const [displays, windows, cameras, microphones, tracks, defaultOutputDirectory] =
    await Promise.all([
      screenAllowed ? settle(api.displays(), [], errors) : Promise.resolve([]),
      screenAllowed ? settle(api.windows(), [], errors) : Promise.resolve([]),
      settle(api.cameras(), [], errors),
      settle(api.microphones(), [], errors),
      settle(api.musicTracks(), [], errors),
      settle(api.defaultOutputDirectory(), null, errors),
    ]);
  return {
    data: {
      capabilities,
      permissions,
      displays,
      windows,
      cameras,
      microphones,
      tracks,
      defaultOutputDirectory,
    },
    errors,
  };
}

/**
 * Loads capabilities, permissions and devices, and reloads them when the
 * window regains focus (e.g. after the user changed System Settings or
 * plugged in a camera).
 */
export function useBackend() {
  const [data, setData] = useState<BackendData | null>(null);
  const [errors, setErrors] = useState<BackendError[]>([]);
  const [fatal, setFatal] = useState<BackendError | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await loadAll();
      setData(result.data);
      setErrors(result.errors);
      setFatal(null);
    } catch (error) {
      setFatal(toBackendError(error));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  return { data, errors, fatal, refresh };
}
