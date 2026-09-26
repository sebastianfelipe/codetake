import { describe, expect, it } from "vitest";
import { defaultPreset, deserializePreset, type Preset, serializePreset } from "./preset";

const custom: Preset = {
  ...defaultPreset,
  source: { kind: "display", id: 2 },
  camera: {
    enabled: true,
    deviceId: "cam-1",
    size: "large",
    position: "topLeft",
    shape: "roundedRectangle",
  },
  microphone: { enabled: false, deviceId: "mic-1" },
  systemAudio: true,
  music: { trackId: "coding-02", volume: 0.6 },
  resolution: "1440p",
  fps: 60,
  countdown: false,
  outputDirectory: "/Users/dev/Videos",
};

describe("preset serialization", () => {
  it("round-trips every field", () => {
    expect(deserializePreset(serializePreset(custom))).toEqual(custom);
  });

  it("accepts already-parsed JSON from the backend", () => {
    expect(deserializePreset(JSON.parse(serializePreset(custom)))).toEqual(custom);
  });

  it("falls back to defaults for missing or corrupted settings", () => {
    expect(deserializePreset(null)).toEqual(defaultPreset);
    expect(deserializePreset("{not json")).toEqual(defaultPreset);
    expect(deserializePreset([])).toEqual(defaultPreset);
    expect(deserializePreset({})).toEqual(defaultPreset);
  });

  it("repairs invalid fields individually", () => {
    const preset = deserializePreset({
      ...custom,
      fps: 24,
      resolution: "8k",
      camera: { ...custom.camera, position: "middle", size: 3 },
      music: { trackId: "", volume: 7 },
    });
    expect(preset.fps).toBe(30);
    expect(preset.resolution).toBe("source");
    expect(preset.camera.position).toBe("bottomRight");
    expect(preset.camera.size).toBe("medium");
    expect(preset.camera.shape).toBe("roundedRectangle");
    expect(preset.music).toEqual({ trackId: null, volume: 1 });
    expect(preset.systemAudio).toBe(true);
  });

  it("does not restore window sources, whose IDs change between launches", () => {
    const preset = deserializePreset({ ...custom, source: { kind: "window", id: 99 } });
    expect(preset.source).toBeNull();
  });

  it("rejects malformed sources", () => {
    expect(deserializePreset({ source: { kind: "display", id: "2" } }).source).toBeNull();
    expect(deserializePreset({ source: { kind: "tab", id: 2 } }).source).toBeNull();
  });
});
