import { describe, expect, it } from "vitest";
import type { PlatformCapabilities } from "../../types/backend";
import { defaultPreset, type Preset } from "./preset";
import {
  buildRecordingConfig,
  resolvePreset,
  type SetupContext,
  validateSetup,
} from "./validation";

const yes = { supported: true, reason: null };

const capabilities: PlatformCapabilities = {
  os: "macos",
  osVersion: "15.0.0",
  recording: yes,
  displayCapture: yes,
  windowCapture: yes,
  camera: yes,
  microphone: yes,
  systemAudio: yes,
};

const ctx: SetupContext = {
  capabilities,
  permissions: { screenRecording: "granted", camera: "granted", microphone: "granted" },
  displays: [
    { id: 1, name: "Built-in", width: 2880, height: 1800, refreshRate: 120, isPrimary: false },
    { id: 2, name: "Studio", width: 3840, height: 2160, refreshRate: 60, isPrimary: true },
  ],
  windows: [{ id: 50, title: "main.rs", appName: "Code", width: 1600, height: 1000 }],
  cameras: [
    { id: "cam-a", name: "FaceTime", isDefault: false },
    { id: "cam-b", name: "Brio", isDefault: true },
  ],
  microphones: [{ id: "mic-a", name: "MacBook Microphone", isDefault: true }],
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
  availableResolutions: ["source", "1080p", "1440p", "2160p"],
  defaultOutputDirectory: "/Users/dev/Movies/CodeTake",
};

const ready: Preset = resolvePreset(defaultPreset, ctx);

describe("resolvePreset", () => {
  it("defaults to the primary display and default devices", () => {
    expect(ready.source).toEqual({ kind: "display", id: 2 });
    expect(ready.camera.deviceId).toBe("cam-b");
    expect(ready.microphone.deviceId).toBe("mic-a");
  });

  it("keeps valid choices and replaces disconnected ones", () => {
    const preset = resolvePreset(
      {
        ...defaultPreset,
        source: { kind: "display", id: 1 },
        camera: { ...defaultPreset.camera, deviceId: "cam-a" },
        microphone: { enabled: true, deviceId: "unplugged" },
        music: { trackId: "deleted", volume: 0.3 },
      },
      ctx,
    );
    expect(preset.source).toEqual({ kind: "display", id: 1 });
    expect(preset.camera.deviceId).toBe("cam-a");
    expect(preset.microphone.deviceId).toBe("mic-a");
    expect(preset.music.trackId).toBeNull();
  });
});

describe("validateSetup", () => {
  it("accepts a complete setup", () => {
    expect(validateSetup(ready, ctx)).toEqual([]);
  });

  it("stops early on unsupported platforms and explains why", () => {
    const issues = validateSetup(ready, {
      ...ctx,
      capabilities: {
        ...capabilities,
        recording: { supported: false, reason: "Recording on linux is not implemented yet." },
      },
    });
    expect(issues).toEqual([
      { field: "platform", message: "Recording on linux is not implemented yet." },
    ]);
  });

  it("asks for missing permissions", () => {
    const issues = validateSetup(ready, {
      ...ctx,
      permissions: { screenRecording: "notDetermined", camera: "denied", microphone: "granted" },
    });
    expect(issues.map((i) => i.permission)).toEqual(["screenRecording", "camera"]);
  });

  it("does not require camera or microphone permission when they are off", () => {
    const preset: Preset = {
      ...ready,
      camera: { ...ready.camera, enabled: false },
      microphone: { ...ready.microphone, enabled: false },
    };
    const issues = validateSetup(preset, {
      ...ctx,
      permissions: { screenRecording: "granted", camera: "denied", microphone: "denied" },
    });
    expect(issues).toEqual([]);
  });

  it("reports missing devices and sources", () => {
    const issues = validateSetup(
      { ...ready, source: { kind: "window", id: 999 } },
      { ...ctx, cameras: [], microphones: [] },
    );
    expect(issues.map((i) => i.field)).toEqual(["source", "camera", "microphone"]);
  });

  it("rejects system audio where it is unsupported", () => {
    const issues = validateSetup(
      { ...ready, systemAudio: true },
      {
        ...ctx,
        capabilities: {
          ...capabilities,
          systemAudio: { supported: false, reason: "Needs macOS 13." },
        },
      },
    );
    expect(issues).toEqual([{ field: "systemAudio", message: "Needs macOS 13." }]);
  });

  it("rejects resolutions that would upscale", () => {
    const issues = validateSetup(
      { ...ready, resolution: "2160p" },
      { ...ctx, availableResolutions: ["source", "1080p"] },
    );
    expect(issues.map((i) => i.field)).toEqual(["resolution"]);
  });

  it("requires an output directory", () => {
    const issues = validateSetup(ready, { ...ctx, defaultOutputDirectory: null });
    expect(issues.map((i) => i.field)).toEqual(["output"]);
  });
});

describe("buildRecordingConfig", () => {
  it("builds the backend config from a valid preset", () => {
    const config = buildRecordingConfig(
      { ...ready, music: { trackId: "coding-01", volume: 0.4 }, fps: 60 },
      ctx,
    );
    expect(config).toEqual({
      source: { kind: "display", id: 2 },
      camera: {
        deviceId: "cam-b",
        overlay: { size: "medium", position: "bottomRight", shape: "circle" },
      },
      microphone: { deviceId: "mic-a" },
      systemAudio: false,
      music: { trackId: "coding-01", volume: 0.4 },
      resolution: "source",
      fps: 60,
      outputDirectory: "/Users/dev/Movies/CodeTake",
    });
  });

  it("omits disabled sources and prefers a custom output directory", () => {
    const config = buildRecordingConfig(
      {
        ...ready,
        camera: { ...ready.camera, enabled: false },
        microphone: { ...ready.microphone, enabled: false },
        outputDirectory: "/Volumes/External/Recordings",
      },
      ctx,
    );
    expect(config?.camera).toBeNull();
    expect(config?.microphone).toBeNull();
    expect(config?.outputDirectory).toBe("/Volumes/External/Recordings");
  });

  it("returns null for an invalid setup", () => {
    expect(buildRecordingConfig({ ...ready, source: null }, ctx)).toBeNull();
  });
});
