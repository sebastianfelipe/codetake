// Types mirroring the Rust backend (src-tauri/src). Keep in sync with the
// serde representations there.

export type PermissionKind = "screenRecording" | "camera" | "microphone";

export type PermissionStatus =
  | "granted"
  | "denied"
  | "restricted"
  | "notDetermined"
  | "notRequired";

export interface PermissionState {
  screenRecording: PermissionStatus;
  camera: PermissionStatus;
  microphone: PermissionStatus;
}

export interface Support {
  supported: boolean;
  reason: string | null;
}

export interface PlatformCapabilities {
  os: "macos" | "windows" | "linux" | string;
  osVersion: string;
  recording: Support;
  displayCapture: Support;
  windowCapture: Support;
  camera: Support;
  microphone: Support;
  systemAudio: Support;
}

export interface DisplayInfo {
  id: number;
  name: string;
  width: number;
  height: number;
  refreshRate: number | null;
  isPrimary: boolean;
}

export interface WindowInfo {
  id: number;
  title: string;
  appName: string;
  width: number;
  height: number;
}

export interface CameraInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface MicrophoneInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface MusicTrack {
  id: string;
  title: string;
  file: string;
  author: string;
  license: string;
  description: string;
}

export type CaptureSource = { kind: "display"; id: number } | { kind: "window"; id: number };

export type OverlayPosition = "topLeft" | "topRight" | "bottomLeft" | "bottomRight";
export type OverlayShape = "circle" | "roundedRectangle";
export type OverlaySize = "small" | "medium" | "large";
export type Resolution = "source" | "1080p" | "1440p" | "2160p";
export type Fps = 30 | 60;

export interface CameraOverlay {
  size: OverlaySize;
  position: OverlayPosition;
  shape: OverlayShape;
}

/** Sent to `start_recording`; mirrors `config::RecordingConfig`. */
export interface RecordingConfig {
  source: CaptureSource;
  camera: { deviceId: string; overlay: CameraOverlay } | null;
  microphone: { deviceId: string } | null;
  systemAudio: boolean;
  music: { trackId: string; volume: number } | null;
  resolution: Resolution;
  fps: Fps;
  outputDirectory: string;
}

export type BackendRecordingState = "idle" | "starting" | "recording" | "paused" | "stopping";

export interface RecordingStatus {
  state: BackendRecordingState;
  elapsedMs: number;
  microphoneLevel: number;
  systemAudioLevel: number;
  warnings: string[];
}

export interface BackendError {
  code: string;
  message: string;
  permission: PermissionKind | null;
}

export interface RecordingOutcome {
  path: string | null;
  durationMs: number;
  complete: boolean;
  error: BackendError | null;
  warnings: string[];
}

export interface SessionSummary {
  outputPath: string;
  startedAt: string;
  outputSize: { width: number; height: number };
}

export type PreviewKind = "screen" | "camera" | "microphone";
