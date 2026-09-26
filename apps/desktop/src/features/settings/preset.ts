// The user's saved recording setup.
//
// Presets are persisted as JSON by the backend. Loading is defensive: every
// field is validated individually, so a corrupted or older settings file
// falls back to defaults field by field instead of breaking the app.

import type {
  CaptureSource,
  Fps,
  OverlayPosition,
  OverlayShape,
  OverlaySize,
  Resolution,
} from "../../types/backend";

export const PRESET_VERSION = 1;

export interface Preset {
  version: typeof PRESET_VERSION;
  source: CaptureSource | null;
  camera: {
    enabled: boolean;
    deviceId: string | null;
    size: OverlaySize;
    position: OverlayPosition;
    shape: OverlayShape;
  };
  microphone: { enabled: boolean; deviceId: string | null };
  systemAudio: boolean;
  music: { trackId: string | null; volume: number };
  resolution: Resolution;
  fps: Fps;
  countdown: boolean;
  outputDirectory: string | null;
}

export const defaultPreset: Preset = {
  version: PRESET_VERSION,
  source: null,
  camera: {
    enabled: true,
    deviceId: null,
    size: "medium",
    position: "bottomRight",
    shape: "circle",
  },
  microphone: { enabled: true, deviceId: null },
  systemAudio: false,
  music: { trackId: null, volume: 0.25 },
  resolution: "source",
  fps: 30,
  countdown: true,
  outputDirectory: null,
};

const RESOLUTIONS: readonly Resolution[] = ["source", "1080p", "1440p", "2160p"];
const POSITIONS: readonly OverlayPosition[] = ["topLeft", "topRight", "bottomLeft", "bottomRight"];
const SHAPES: readonly OverlayShape[] = ["circle", "roundedRectangle"];
const SIZES: readonly OverlaySize[] = ["small", "medium", "large"];

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function oneOf<T>(value: unknown, options: readonly T[], fallback: T): T {
  return options.includes(value as T) ? (value as T) : fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function parseSource(value: unknown): CaptureSource | null {
  if (!isObject(value) || typeof value.id !== "number" || !Number.isInteger(value.id)) {
    return null;
  }
  if (value.kind === "display" || value.kind === "window") {
    return { kind: value.kind, id: value.id };
  }
  return null;
}

function clampVolume(value: unknown, fallback: number): number {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return fallback;
  }
  return Math.min(1, Math.max(0, value));
}

export function serializePreset(preset: Preset): string {
  return JSON.stringify(preset);
}

/** Restores a preset from saved JSON (string or parsed), filling gaps with defaults. */
export function deserializePreset(input: unknown): Preset {
  let data: unknown = input;
  if (typeof input === "string") {
    try {
      data = JSON.parse(input);
    } catch {
      return defaultPreset;
    }
  }
  if (!isObject(data)) {
    return defaultPreset;
  }
  const d = defaultPreset;
  const camera = isObject(data.camera) ? data.camera : {};
  const microphone = isObject(data.microphone) ? data.microphone : {};
  const music = isObject(data.music) ? data.music : {};

  return {
    version: PRESET_VERSION,
    // Window IDs don't survive app restarts, so only displays are restored.
    source: (() => {
      const source = parseSource(data.source);
      return source?.kind === "display" ? source : null;
    })(),
    camera: {
      enabled: bool(camera.enabled, d.camera.enabled),
      deviceId: optionalString(camera.deviceId),
      size: oneOf(camera.size, SIZES, d.camera.size),
      position: oneOf(camera.position, POSITIONS, d.camera.position),
      shape: oneOf(camera.shape, SHAPES, d.camera.shape),
    },
    microphone: {
      enabled: bool(microphone.enabled, d.microphone.enabled),
      deviceId: optionalString(microphone.deviceId),
    },
    systemAudio: bool(data.systemAudio, d.systemAudio),
    music: {
      trackId: optionalString(music.trackId),
      volume: clampVolume(music.volume, d.music.volume),
    },
    resolution: oneOf(data.resolution, RESOLUTIONS, d.resolution),
    fps: oneOf<Fps>(data.fps, [30, 60], d.fps),
    countdown: bool(data.countdown, d.countdown),
    outputDirectory: optionalString(data.outputDirectory),
  };
}
