// What the menu bar icon shows, derived from the UI state. The backend only
// renders it (src-tauri/src/tray.rs).

import { formatDuration, formatResolution } from "../../lib/format";
import type { RecordingView } from "../recording/machine";
import type { Preset } from "../settings/preset";
import { RESOLUTION_LABELS } from "../settings/resolution";
import type { Issue, SetupContext } from "../settings/validation";

export type TrayPhase = "idle" | "countdown" | "starting" | "recording" | "paused" | "saving";

export interface TrayState {
  phase: TrayPhase;
  canRecord: boolean;
  blockedReason: string | null;
  details: string[];
}

export function trayPhase(view: RecordingView): TrayPhase {
  switch (view.phase) {
    case "idle":
    case "finished":
      return "idle";
    case "stopping":
      return "saving";
    default:
      return view.phase;
  }
}

/** Text next to the menu bar icon, e.g. "● 00:12:42". */
export function trayTitle(view: RecordingView): string | null {
  switch (view.phase) {
    case "countdown":
      return `Recording in ${view.remaining}…`;
    case "starting":
      return "Starting…";
    case "recording":
      return `● ${formatDuration(view.elapsedMs)}`;
    case "paused":
      return `❚❚ ${formatDuration(view.elapsedMs)}`;
    case "stopping":
      return "Saving…";
    default:
      return null;
  }
}

const SHAPE_LABELS = { circle: "Circle", square: "Square", rectangle: "Wide" } as const;

/** One line per setting, so the user can see what will be recorded. */
export function trayDetails(
  preset: Preset,
  ctx: Pick<SetupContext, "displays" | "windows" | "cameras" | "microphones" | "tracks">,
): string[] {
  const source = preset.source;
  let screen = "Screen: Not selected";
  if (source?.kind === "display") {
    const display = ctx.displays.find((d) => d.id === source.id);
    if (display)
      screen = `Screen: ${display.name} (${formatResolution(display.width, display.height)})`;
  } else if (source?.kind === "window") {
    const window = ctx.windows.find((w) => w.id === source.id);
    if (window) screen = `Window: ${window.appName} — ${window.title}`.slice(0, 70);
  }

  const cameraName = ctx.cameras.find((c) => c.id === preset.camera.deviceId)?.name;
  const camera = preset.camera.enabled
    ? `Camera: ${cameraName ?? "Not available"} · ${SHAPE_LABELS[preset.camera.shape]}`
    : "Camera: Off";

  const microphoneName = ctx.microphones.find((m) => m.id === preset.microphone.deviceId)?.name;
  const microphone = preset.microphone.enabled
    ? `Microphone: ${microphoneName ?? "Not available"}`
    : "Microphone: Off";

  const track = ctx.tracks.find((t) => t.id === preset.music.trackId);
  return [
    screen,
    camera,
    microphone,
    `System audio: ${preset.systemAudio ? "On" : "Off"}`,
    `Music: ${track ? `${track.title} (${Math.round(preset.music.volume * 100)}%)` : "None"}`,
    `Video: ${RESOLUTION_LABELS[preset.resolution]} · ${preset.fps} FPS`,
  ];
}

export function buildTrayState(
  view: RecordingView,
  preset: Preset,
  ctx: Pick<SetupContext, "displays" | "windows" | "cameras" | "microphones" | "tracks">,
  issues: Issue[],
): TrayState {
  return {
    phase: trayPhase(view),
    canRecord: issues.length === 0,
    blockedReason: issues[0]?.message ?? null,
    details: trayDetails(preset, ctx),
  };
}
