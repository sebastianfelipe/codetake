// A fake backend for developing the UI in a regular browser (`pnpm dev`,
// then open http://localhost:1420). Only loaded in development when the page
// is not running inside Tauri. It never records anything.

import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { mockConvertFileSrc, mockIPC } from "@tauri-apps/api/mocks";
import type {
  PermissionState,
  PlatformCapabilities,
  RecordingOutcome,
  RecordingStatus,
} from "../types/backend";

const yes = { supported: true, reason: null };

const capabilities: PlatformCapabilities = {
  os: "macos",
  osVersion: "15.0.0 (mock)",
  recording: yes,
  displayCapture: yes,
  windowCapture: yes,
  camera: yes,
  microphone: yes,
  systemAudio: yes,
};

// Start with camera permission missing so the permission panel is visible.
const permissions: PermissionState = {
  screenRecording: "granted",
  camera: "notDetermined",
  microphone: "granted",
};

let settings: unknown = null;
let recording: { startedAt: number; pausedAt: number | null; pausedMs: number } | null = null;
let statusTimer: number | undefined;

function elapsed(): number {
  if (!recording) return 0;
  const end = recording.pausedAt ?? Date.now();
  return end - recording.startedAt - recording.pausedMs;
}

function status(): RecordingStatus {
  return {
    state: recording?.pausedAt ? "paused" : "recording",
    elapsedMs: elapsed(),
    microphoneLevel: Math.random() * 0.4,
    systemAudioLevel: 0,
    warnings: [],
  };
}

/** Encodes RGBA pixels in the binary preview format. */
function encodeFrame(kind: 0 | 1, image: ImageData): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + image.data.length);
  const header = new DataView(buffer, 0, 8);
  header.setUint8(0, kind);
  header.setUint16(2, image.width, true);
  header.setUint16(4, image.height, true);
  new Uint8ClampedArray(buffer, 8).set(image.data);
  return buffer;
}

/** A synthetic code editor, drawn with shapes (no real screen content). */
function sampleScreen(width: number, height: number): ImageData {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d") as OffscreenCanvasRenderingContext2D;
  ctx.fillStyle = "#1b1e28";
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = "#15171f";
  ctx.fillRect(0, 0, width * 0.18, height);
  ctx.fillStyle = "#232735";
  ctx.fillRect(0, 0, width, height * 0.05);
  const colors = ["#7aa2f7", "#bb9af7", "#9ece6a", "#e0af68", "#7dcfff", "#c0caf5"];
  let seed = 7;
  const random = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };
  for (let row = 0; row < 26; row++) {
    const y = height * 0.08 + row * height * 0.034;
    let x = width * 0.22 + (row % 5 === 0 ? 0 : (1 + Math.floor(random() * 3)) * width * 0.025);
    for (let token = 0; token < 1 + Math.floor(random() * 5); token++) {
      const w = width * (0.03 + random() * 0.09);
      ctx.fillStyle = colors[Math.floor(random() * colors.length)] ?? "#c0caf5";
      ctx.globalAlpha = 0.85;
      ctx.fillRect(x, y, w, height * 0.014);
      x += w + width * 0.012;
    }
    ctx.globalAlpha = 0.35;
    ctx.fillStyle = "#c0caf5";
    ctx.fillRect(width * 0.02, y, width * (0.06 + random() * 0.08), height * 0.012);
  }
  ctx.globalAlpha = 1;
  return ctx.getImageData(0, 0, width, height);
}

/** A soft placeholder where the webcam image would be. */
function sampleCamera(width: number, height: number): ImageData {
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext("2d") as OffscreenCanvasRenderingContext2D;
  const gradient = ctx.createLinearGradient(0, 0, width, height);
  gradient.addColorStop(0, "#3fd8ff");
  gradient.addColorStop(1, "#8a5cff");
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = "rgba(255, 255, 255, 0.85)";
  ctx.beginPath();
  ctx.arc(width / 2, height * 0.42, height * 0.17, 0, Math.PI * 2);
  ctx.fill();
  ctx.beginPath();
  ctx.ellipse(width / 2, height * 0.95, width * 0.3, height * 0.3, 0, 0, Math.PI * 2);
  ctx.fill();
  return ctx.getImageData(0, 0, width, height);
}

export function installMockBackend(): void {
  mockConvertFileSrc("macos");
  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_capabilities":
          return capabilities;
        case "get_permissions":
          return permissions;
        case "request_permission": {
          const kind = payload.kind as keyof PermissionState;
          permissions[kind] = "granted";
          return "granted";
        }
        case "open_permission_settings":
          return null;
        case "list_displays":
          return [
            {
              id: 1,
              name: "Built-in Display",
              width: 3024,
              height: 1964,
              refreshRate: 120,
              isPrimary: true,
            },
            {
              id: 2,
              name: "Display 2",
              width: 3840,
              height: 2160,
              refreshRate: 60,
              isPrimary: false,
            },
          ];
        case "list_windows":
          return [
            { id: 77, title: "main.rs — codetake", appName: "Code", width: 2400, height: 1500 },
          ];
        case "list_cameras":
          return [{ id: "cam", name: "FaceTime HD Camera", isDefault: true }];
        case "list_microphones":
          return [{ id: "mic", name: "MacBook Pro Microphone", isDefault: true }];
        case "list_music_tracks":
          return [
            {
              id: "coding-01",
              title: "Late Night Commit",
              file: "coding-01.m4a",
              author: "CodeTake contributors",
              license: "CC0-1.0",
              description: "",
            },
          ];
        case "default_output_directory":
          return "/Users/dev/Movies/CodeTake";
        case "recover_recordings":
          return [];
        case "load_settings":
          return settings;
        case "save_settings":
          settings = payload.settings;
          return null;
        case "plugin:path|resolve_directory":
          return "/Users/dev";
        case "start_preview": {
          const channel = payload.frames as Channel<ArrayBuffer> | undefined;
          window.setTimeout(() => {
            channel?.onmessage(encodeFrame(0, sampleScreen(960, 624)));
            channel?.onmessage(encodeFrame(1, sampleCamera(320, 240)));
          }, 50);
          return null;
        }
        case "allow_media":
          return null;
        case "music_track_file":
          return `/mock/music/${String(payload.trackId)}.m4a`;
        case "export_video": {
          const progress = payload.progress as Channel<number> | undefined;
          return new Promise((resolve) => {
            let fraction = 0;
            const timer = window.setInterval(() => {
              fraction = Math.min(1, fraction + 0.25);
              progress?.onmessage(fraction);
              if (fraction >= 1) {
                window.clearInterval(timer);
                resolve(
                  "/Users/dev/Movies/CodeTake/2026-09-26/coding-session-2026-09-26-09-32-14.mp4",
                );
              }
            }, 150);
          });
        }
        case "stop_preview":
        case "update_tray":
        case "set_tray_indicator":
          return null;
        case "start_recording":
          recording = { startedAt: Date.now(), pausedAt: null, pausedMs: 0 };
          statusTimer = window.setInterval(() => void emit("recording://status", status()), 250);
          return {
            outputPath: "/Users/dev/Movies/CodeTake/mock.mp4",
            startedAt: "",
            outputSize: { width: 3024, height: 1964 },
          };
        case "pause_recording":
          if (recording) recording.pausedAt = Date.now();
          return null;
        case "resume_recording":
          if (recording?.pausedAt) {
            recording.pausedMs += Date.now() - recording.pausedAt;
            recording.pausedAt = null;
          }
          return null;
        case "stop_recording": {
          window.clearInterval(statusTimer);
          const outcome: RecordingOutcome = {
            path: "/Users/dev/Movies/CodeTake/2026-09-26/raw/coding-session-2026-09-26-09-32-14-screen.mp4",
            cameraPath:
              "/Users/dev/Movies/CodeTake/2026-09-26/raw/coding-session-2026-09-26-09-32-14-camera.mp4",
            exportPath:
              "/Users/dev/Movies/CodeTake/2026-09-26/coding-session-2026-09-26-09-32-14.mp4",
            overlay: { shape: "circle", size: 0.25, x: 0.91, y: 0.84 },
            fps: 30,
            durationMs: elapsed(),
            complete: true,
            error: null,
            warnings: [],
          };
          recording = null;
          return outcome;
        }
        default:
          console.warn(`mock backend: unhandled command ${cmd}`);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
