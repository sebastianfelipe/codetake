// The user's saved recording setup.
//
// Presets are persisted as JSON by the backend. Loading is defensive: every
// field is validated individually, so a corrupted or older settings file
// falls back to defaults field by field instead of breaking the app.

import type { CaptureSource, Fps, OverlayShape, Resolution } from "../../types/backend";
import { OVERLAY_MAX_SIZE, OVERLAY_MIN_SIZE, OVERLAY_PRESETS } from "../preview/overlay";

export const PRESET_VERSION = 1;

export interface Preset {
  version: typeof PRESET_VERSION;
  source: CaptureSource | null;
  camera: {
    enabled: boolean;
    deviceId: string | null;
    shape: OverlayShape;
    /** Height as a fraction of the video height. */
    size: number;
    /** Center of the overlay as fractions of the frame. */
    x: number;
    y: number;
  };
  microphone: { enabled: boolean; deviceId: string | null };
  systemAudio: boolean;
  /** Defaults for the review step, where music is added after recording. */
  music: { trackId: string | null; volume: number; recordingVolume: number };
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
    shape: "circle",
    size: 0.25,
    ...OVERLAY_PRESETS.bottomRight,
  },
  microphone: { enabled: true, deviceId: null },
  systemAudio: false,
  music: { trackId: null, volume: 0.25, recordingVolume: 1 },
  resolution: "source",
  fps: 30,
  countdown: true,
  outputDirectory: null,
};

const RESOLUTIONS: readonly Resolution[] = ["source", "1080p", "1440p", "2160p"];
const SHAPES: readonly OverlayShape[] = ["circle", "square", "rectangle"];

// Presets saved before the overlay could be moved and resized freely.
const LEGACY_SIZES: Record<string, number> = { small: 0.18, medium: 0.25, large: 0.34 };
const LEGACY_SHAPES: Record<string, OverlayShape> = { roundedRectangle: "rectangle" };

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

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, value));
}

function parseCamera(camera: Json): Preset["camera"] {
  const d = defaultPreset.camera;
  const legacyPosition =
    typeof camera.position === "string" && camera.position in OVERLAY_PRESETS
      ? OVERLAY_PRESETS[camera.position as keyof typeof OVERLAY_PRESETS]
      : null;
  const size =
    typeof camera.size === "string" ? (LEGACY_SIZES[camera.size] ?? d.size) : camera.size;
  const shape =
    typeof camera.shape === "string" && camera.shape in LEGACY_SHAPES
      ? LEGACY_SHAPES[camera.shape]
      : camera.shape;
  return {
    enabled: bool(camera.enabled, d.enabled),
    deviceId: optionalString(camera.deviceId),
    shape: oneOf(shape, SHAPES, d.shape),
    size: clampNumber(size, OVERLAY_MIN_SIZE, OVERLAY_MAX_SIZE, d.size),
    x: clampNumber(camera.x, 0, 1, legacyPosition?.x ?? d.x),
    y: clampNumber(camera.y, 0, 1, legacyPosition?.y ?? d.y),
  };
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
    camera: parseCamera(camera),
    microphone: {
      enabled: bool(microphone.enabled, d.microphone.enabled),
      deviceId: optionalString(microphone.deviceId),
    },
    systemAudio: bool(data.systemAudio, d.systemAudio),
    music: {
      trackId: optionalString(music.trackId),
      volume: clampNumber(music.volume, 0, 1, d.music.volume),
      recordingVolume: clampNumber(music.recordingVolume, 0, 2, d.music.recordingVolume),
    },
    resolution: oneOf(data.resolution, RESOLUTIONS, d.resolution),
    fps: oneOf<Fps>(data.fps, [30, 60], d.fps),
    countdown: bool(data.countdown, d.countdown),
    outputDirectory: optionalString(data.outputDirectory),
  };
}
