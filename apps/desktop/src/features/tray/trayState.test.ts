import { describe, expect, it } from "vitest";
import { defaultPreset, type Preset } from "../settings/preset";
import { buildTrayState, trayDetails, trayElapsed, trayPhase, trayTitle } from "./trayState";

const ctx = {
  displays: [
    {
      id: 1,
      name: "Built-in Display",
      width: 3024,
      height: 1964,
      refreshRate: 120,
      isPrimary: true,
    },
  ],
  windows: [{ id: 9, title: "main.rs", appName: "Code", width: 1600, height: 1000 }],
  cameras: [{ id: "cam", name: "FaceTime HD Camera", isDefault: true }],
  microphones: [{ id: "mic", name: "MacBook Pro Microphone", isDefault: true }],
  tracks: [
    {
      id: "coding-01",
      title: "Late Night Commit",
      file: "coding-01.m4a",
      author: "CodeTake contributors",
      license: "CC0-1.0",
      description: "",
    },
  ],
};

const preset: Preset = {
  ...defaultPreset,
  source: { kind: "display", id: 1 },
  camera: { ...defaultPreset.camera, deviceId: "cam", shape: "square" },
  microphone: { enabled: true, deviceId: "mic" },
  music: { trackId: "coding-01", volume: 0.3, recordingVolume: 1 },
};

describe("tray state", () => {
  it("maps the recording flow to tray phases", () => {
    expect(trayPhase({ phase: "idle", error: null })).toBe("idle");
    expect(trayPhase({ phase: "recording", elapsedMs: 0 })).toBe("recording");
    expect(trayPhase({ phase: "stopping", elapsedMs: 0 })).toBe("saving");
    expect(
      trayPhase({
        phase: "finished",
        outcome: {
          path: null,
          cameraPath: null,
          exportPath: "/x.mp4",
          overlay: null,
          fps: 30,
          durationMs: 0,
          complete: false,
          error: null,
          warnings: [],
        },
      }),
    ).toBe("idle");
  });

  it("keeps the text next to the icon short", () => {
    expect(trayTitle({ phase: "idle", error: null }, true)).toBeNull();
    expect(trayTitle({ phase: "countdown", remaining: 2 }, true)).toBe("2");
    expect(trayTitle({ phase: "recording", elapsedMs: 762_000 }, true)).toBe("12:42");
    expect(trayTitle({ phase: "paused", elapsedMs: 5_000 }, true)).toBe("❚❚ 0:05");
    expect(trayTitle({ phase: "stopping", elapsedMs: 5_000 }, true)).toBe("Saving…");
  });

  it("can hide the timer, leaving only the recording icon", () => {
    expect(trayTitle({ phase: "recording", elapsedMs: 762_000 }, false)).toBeNull();
    expect(trayTitle({ phase: "paused", elapsedMs: 5_000 }, false)).toBe("❚❚");
    // The time is still shown inside the menu.
    expect(trayElapsed({ phase: "recording", elapsedMs: 762_000 })).toBe("12:42");
    expect(trayElapsed({ phase: "idle", error: null })).toBeNull();
  });

  it("describes what will be recorded", () => {
    expect(trayDetails(preset, ctx)).toEqual([
      "Screen: Built-in Display (3024 × 1964)",
      "Camera: FaceTime HD Camera · Square",
      "Microphone: MacBook Pro Microphone",
      "System audio: Off",
      "Video: Source (native) · 30 FPS",
    ]);
  });

  it("describes window sources and disabled devices", () => {
    const details = trayDetails(
      {
        ...preset,
        source: { kind: "window", id: 9 },
        camera: { ...preset.camera, enabled: false },
        microphone: { enabled: false, deviceId: null },
        music: { trackId: null, volume: 0.3, recordingVolume: 1 },
      },
      ctx,
    );
    expect(details.slice(0, 3)).toEqual([
      "Window: Code — main.rs",
      "Camera: Off",
      "Microphone: Off",
    ]);
  });

  it("only allows recording when the setup is valid, explaining why not", () => {
    const view = { phase: "idle" as const, error: null };
    expect(buildTrayState(view, preset, ctx, [])).toMatchObject({
      canRecord: true,
      blockedReason: null,
    });
    const blocked = buildTrayState(view, preset, ctx, [
      { field: "camera", message: "CodeTake needs Camera permission.", permission: "camera" },
    ]);
    expect(blocked).toMatchObject({
      canRecord: false,
      blockedReason: "CodeTake needs Camera permission.",
    });
  });
});
