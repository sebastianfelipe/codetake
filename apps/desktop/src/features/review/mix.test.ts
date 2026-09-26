import { describe, expect, it } from "vitest";
import { loopPosition, musicEnvelope, percent, previewLevels } from "./mix";

describe("review mix", () => {
  it("fades music in and out like the exporter", () => {
    expect(musicEnvelope(0, 20)).toBe(0);
    expect(musicEnvelope(0.75, 20)).toBeCloseTo(0.5);
    expect(musicEnvelope(5, 20)).toBe(1);
    expect(musicEnvelope(18.5, 20)).toBeCloseTo(0.5);
    expect(musicEnvelope(20, 20)).toBe(0);
    expect(musicEnvelope(25, 20)).toBe(0);
  });

  it("keeps short videos smooth", () => {
    const peak = Math.max(...Array.from({ length: 21 }, (_, i) => musicEnvelope(i / 10, 2)));
    expect(peak).toBeLessThan(1);
    expect(peak).toBeGreaterThan(0.4);
  });

  it("handles unknown durations", () => {
    expect(musicEnvelope(1, Number.NaN)).toBe(0);
    expect(loopPosition(5, 0)).toBe(0);
  });

  it("maps video time into the looping track", () => {
    expect(loopPosition(10, 4)).toBe(2);
    expect(loopPosition(3, 4)).toBe(3);
  });

  it("previews boosted recordings with the same balance", () => {
    expect(previewLevels(1, 0.3)).toEqual({ recording: 1, music: 0.3 });
    const boosted = previewLevels(1.5, 0.3);
    expect(boosted.recording).toBe(1);
    expect(boosted.music).toBeCloseTo(0.2);
    expect(boosted.recording / boosted.music).toBeCloseTo(1.5 / 0.3);
    expect(previewLevels(0, 0)).toEqual({ recording: 0, music: 0 });
  });

  it("formats volumes", () => {
    expect(percent(0.25)).toBe("25%");
    expect(percent(1.5)).toBe("150%");
  });
});
