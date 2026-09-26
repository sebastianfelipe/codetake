// Typed wrappers around the Tauri commands and events.

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BackendError,
  CameraInfo,
  CaptureSource,
  DisplayInfo,
  MicrophoneInfo,
  MusicTrack,
  PermissionKind,
  PermissionState,
  PermissionStatus,
  PlatformCapabilities,
  PreviewKind,
  RecordingConfig,
  RecordingOutcome,
  RecordingStatus,
  Resolution,
  SessionSummary,
  WindowInfo,
} from "../types/backend";

export const api = {
  capabilities: () => invoke<PlatformCapabilities>("get_capabilities"),
  permissions: () => invoke<PermissionState>("get_permissions"),
  requestPermission: (kind: PermissionKind) =>
    invoke<PermissionStatus>("request_permission", { kind }),
  openPermissionSettings: (kind: PermissionKind) =>
    invoke<void>("open_permission_settings", { kind }),
  displays: () => invoke<DisplayInfo[]>("list_displays"),
  windows: () => invoke<WindowInfo[]>("list_windows"),
  cameras: () => invoke<CameraInfo[]>("list_cameras"),
  microphones: () => invoke<MicrophoneInfo[]>("list_microphones"),
  availableResolutions: (width: number, height: number) =>
    invoke<Resolution[]>("available_resolutions", { width, height }),
  musicTracks: () => invoke<MusicTrack[]>("list_music_tracks"),
  defaultOutputDirectory: () => invoke<string>("default_output_directory"),
  recoverRecordings: (outputDirectory: string) =>
    invoke<string[]>("recover_recordings", { outputDirectory }),
  loadSettings: () => invoke<unknown>("load_settings"),
  saveSettings: (settings: unknown) => invoke<void>("save_settings", { settings }),
  startRecording: (config: RecordingConfig) =>
    invoke<SessionSummary>("start_recording", { config }),
  pauseRecording: () => invoke<void>("pause_recording"),
  resumeRecording: () => invoke<void>("resume_recording"),
  stopRecording: () => invoke<RecordingOutcome>("stop_recording"),
  stopPreview: () => invoke<void>("stop_preview"),
};

export interface PreviewRequest {
  source: CaptureSource | null;
  cameraId: string | null;
  microphoneId: string | null;
}

export interface PreviewFrame {
  kind: PreviewKind;
  image: ImageData;
}

/** Decodes a binary preview message: 8-byte header followed by RGBA pixels. */
export function decodePreviewFrame(buffer: ArrayBuffer): PreviewFrame | null {
  if (buffer.byteLength < 8) {
    return null;
  }
  const header = new DataView(buffer, 0, 8);
  const kinds: PreviewKind[] = ["screen", "camera", "microphone"];
  const kind = kinds[header.getUint8(0)];
  const width = header.getUint16(2, true);
  const height = header.getUint16(4, true);
  const pixels = new Uint8ClampedArray(buffer, 8);
  if (!kind || width === 0 || height === 0 || pixels.length !== width * height * 4) {
    return null;
  }
  return { kind, image: new ImageData(pixels, width, height) };
}

export function startPreview(
  request: PreviewRequest,
  onFrame: (frame: PreviewFrame) => void,
): Promise<void> {
  const frames = new Channel<ArrayBuffer>();
  frames.onmessage = (message) => {
    const frame = decodePreviewFrame(message);
    if (frame) {
      onFrame(frame);
    }
  };
  return invoke<void>("start_preview", { request, frames });
}

export const events = {
  status: (handler: (status: RecordingStatus) => void): Promise<UnlistenFn> =>
    listen<RecordingStatus>("recording://status", (e) => handler(e.payload)),
  finished: (handler: (outcome: RecordingOutcome) => void): Promise<UnlistenFn> =>
    listen<RecordingOutcome>("recording://finished", (e) => handler(e.payload)),
  previewLevel: (handler: (level: number) => void): Promise<UnlistenFn> =>
    listen<number>("preview://level", (e) => handler(e.payload)),
  previewError: (
    handler: (payload: { kind: PreviewKind; error: BackendError }) => void,
  ): Promise<UnlistenFn> =>
    listen<{ kind: PreviewKind; error: BackendError }>("preview://error", (e) =>
      handler(e.payload),
    ),
  toggleShortcut: (handler: () => void): Promise<UnlistenFn> =>
    listen("shortcut://toggle-recording", () => handler()),
};

/** Normalizes anything thrown by `invoke` into a backend error. */
export function toBackendError(error: unknown): BackendError {
  if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = error as Partial<BackendError>;
    return {
      code: typeof candidate.code === "string" ? candidate.code : "unknown",
      message: String(candidate.message),
      permission: candidate.permission ?? null,
    };
  }
  return { code: "unknown", message: String(error), permission: null };
}
