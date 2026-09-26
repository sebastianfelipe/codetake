import { describe, expect, it } from "vitest";
import { formatDuration, formatRefreshRate, formatResolution } from "./format";

describe("formatting", () => {
  it("formats durations as HH:MM:SS", () => {
    expect(formatDuration(0)).toBe("00:00:00");
    expect(formatDuration(999)).toBe("00:00:00");
    expect(formatDuration(762_000)).toBe("00:12:42");
    expect(formatDuration(3_723_000)).toBe("01:02:03");
    expect(formatDuration(-5)).toBe("00:00:00");
  });

  it("formats display details", () => {
    expect(formatResolution(3840, 2160)).toBe("3840 × 2160");
    expect(formatRefreshRate(119.88)).toBe("120 Hz");
    expect(formatRefreshRate(null)).toBeNull();
  });
});
