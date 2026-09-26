// A fake backend for developing the UI in a regular browser (`pnpm dev`,
// then open http://localhost:1420). Only loaded in development when the page
// is not running inside Tauri. It never records anything.

import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
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

/** A gradient "screen" and a flat "camera" frame, in the binary preview format. */
function frame(kind: 0 | 1, width: number, height: number): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + width * height * 4);
  const header = new DataView(buffer, 0, 8);
  header.setUint8(0, kind);
  header.setUint16(2, width, true);
  header.setUint16(4, height, true);
  const pixels = new Uint8ClampedArray(buffer, 8);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      pixels[i] = kind === 0 ? 20 + (x / width) * 40 : 90;
      pixels[i + 1] = kind === 0 ? 24 + (y / height) * 40 : 140;
      pixels[i + 2] = kind === 0 ? 60 : 200;
      pixels[i + 3] = 255;
    }
  }
  return buffer;
}

export function installMockBackend(): void {
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
            channel?.onmessage(frame(0, 480, 312));
            channel?.onmessage(frame(1, 160, 120));
          }, 50);
          return null;
        }
        case "stop_preview":
        case "update_tray":
        case "set_tray_title":
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
            path: "/Users/dev/Movies/CodeTake/2026-09-26/coding-session-2026-09-26-09-32-14.mp4",
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
