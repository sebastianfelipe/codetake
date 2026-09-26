// Checks a preset against what is actually available right now (devices,
// permissions, platform support) and turns it into a recording config.

import type {
  CameraInfo,
  DisplayInfo,
  MicrophoneInfo,
  MusicTrack,
  PermissionKind,
  PermissionState,
  PlatformCapabilities,
  RecordingConfig,
  Resolution,
  WindowInfo,
} from "../../types/backend";
import type { Preset } from "./preset";

export interface SetupContext {
  capabilities: PlatformCapabilities;
  permissions: PermissionState;
  displays: DisplayInfo[];
  windows: WindowInfo[];
  cameras: CameraInfo[];
  microphones: MicrophoneInfo[];
  tracks: MusicTrack[];
  /** Resolutions meaningful for the selected source. */
  availableResolutions: Resolution[];
  defaultOutputDirectory: string | null;
}

export type IssueField =
  | "platform"
  | "source"
  | "camera"
  | "microphone"
  | "systemAudio"
  | "music"
  | "resolution"
  | "output";

export interface Issue {
  field: IssueField;
  message: string;
  /** Set when granting this permission would fix the issue. */
  permission?: PermissionKind;
}

function usable(status: PermissionState[PermissionKind]): boolean {
  return status === "granted" || status === "notRequired";
}

/**
 * Fills in sensible defaults for anything not chosen yet (or no longer
 * available): the primary display, the system default camera/microphone.
 */
export function resolvePreset(
  preset: Preset,
  ctx: Pick<SetupContext, "displays" | "windows" | "cameras" | "microphones" | "tracks">,
): Preset {
  const sourceExists =
    preset.source?.kind === "display"
      ? ctx.displays.some((d) => d.id === preset.source?.id)
      : preset.source?.kind === "window"
        ? ctx.windows.some((w) => w.id === preset.source?.id)
        : false;
  const primary = ctx.displays.find((d) => d.isPrimary) ?? ctx.displays[0];
  const source = sourceExists
    ? preset.source
    : primary
      ? { kind: "display" as const, id: primary.id }
      : null;

  const pick = <T extends { id: string; isDefault: boolean }>(
    list: T[],
    id: string | null,
  ): string | null =>
    list.find((d) => d.id === id)?.id ?? list.find((d) => d.isDefault)?.id ?? list[0]?.id ?? null;

  return {
    ...preset,
    source,
    camera: { ...preset.camera, deviceId: pick(ctx.cameras, preset.camera.deviceId) },
    microphone: {
      ...preset.microphone,
      deviceId: pick(ctx.microphones, preset.microphone.deviceId),
    },
    music: {
      ...preset.music,
      trackId: ctx.tracks.some((t) => t.id === preset.music.trackId) ? preset.music.trackId : null,
    },
  };
}

export function validateSetup(preset: Preset, ctx: SetupContext): Issue[] {
  const issues: Issue[] = [];
  const caps = ctx.capabilities;

  if (!caps.recording.supported) {
    issues.push({
      field: "platform",
      message: caps.recording.reason ?? "Recording is not supported on this system yet.",
    });
    return issues;
  }

  if (!usable(ctx.permissions.screenRecording)) {
    issues.push({
      field: "source",
      message: "CodeTake needs Screen Recording permission to capture your screen.",
      permission: "screenRecording",
    });
  } else if (!preset.source) {
    issues.push({ field: "source", message: "Choose a display or window to record." });
  } else if (
    preset.source.kind === "display"
      ? !ctx.displays.some((d) => d.id === preset.source?.id)
      : !ctx.windows.some((w) => w.id === preset.source?.id)
  ) {
    issues.push({
      field: "source",
      message: "The selected screen is no longer available. Choose another one.",
    });
  } else if (preset.source.kind === "window" && !caps.windowCapture.supported) {
    issues.push({
      field: "source",
      message: caps.windowCapture.reason ?? "Window capture is not supported here.",
    });
  }

  if (preset.camera.enabled) {
    if (!caps.camera.supported) {
      issues.push({ field: "camera", message: caps.camera.reason ?? "Camera is not supported." });
    } else if (!usable(ctx.permissions.camera)) {
      issues.push({
        field: "camera",
        message: "CodeTake needs Camera permission to show your webcam.",
        permission: "camera",
      });
    } else if (!ctx.cameras.some((c) => c.id === preset.camera.deviceId)) {
      issues.push({
        field: "camera",
        message: "No camera is available. Connect one or turn the camera off.",
      });
    }
  }

  if (preset.microphone.enabled) {
    if (!caps.microphone.supported) {
      issues.push({
        field: "microphone",
        message: caps.microphone.reason ?? "Microphone is not supported.",
      });
    } else if (!usable(ctx.permissions.microphone)) {
      issues.push({
        field: "microphone",
        message: "CodeTake needs Microphone permission to record your voice.",
        permission: "microphone",
      });
    } else if (!ctx.microphones.some((m) => m.id === preset.microphone.deviceId)) {
      issues.push({
        field: "microphone",
        message: "No microphone is available. Connect one or turn the microphone off.",
      });
    }
  }

  if (preset.systemAudio && !caps.systemAudio.supported) {
    issues.push({
      field: "systemAudio",
      message: caps.systemAudio.reason ?? "System audio capture is not supported on this system.",
    });
  }

  if (preset.music.trackId && !ctx.tracks.some((t) => t.id === preset.music.trackId)) {
    issues.push({ field: "music", message: "The selected music track is missing." });
  }

  if (!ctx.availableResolutions.includes(preset.resolution)) {
    issues.push({
      field: "resolution",
      message: "The selected screen is smaller than this resolution; CodeTake never upscales.",
    });
  }

  if (!(preset.outputDirectory ?? ctx.defaultOutputDirectory)) {
    issues.push({ field: "output", message: "Choose where recordings are saved." });
  }

  return issues;
}

/** Builds the config sent to the backend, or `null` if the setup is invalid. */
export function buildRecordingConfig(preset: Preset, ctx: SetupContext): RecordingConfig | null {
  const outputDirectory = preset.outputDirectory ?? ctx.defaultOutputDirectory;
  if (validateSetup(preset, ctx).length > 0 || !preset.source || !outputDirectory) {
    return null;
  }
  const { camera, microphone, music } = preset;
  return {
    source: preset.source,
    camera:
      camera.enabled && camera.deviceId
        ? {
            deviceId: camera.deviceId,
            overlay: { shape: camera.shape, size: camera.size, x: camera.x, y: camera.y },
          }
        : null,
    microphone:
      microphone.enabled && microphone.deviceId ? { deviceId: microphone.deviceId } : null,
    systemAudio: preset.systemAudio,
    music: music.trackId ? { trackId: music.trackId, volume: music.volume } : null,
    resolution: preset.resolution,
    fps: preset.fps,
    outputDirectory,
  };
}
