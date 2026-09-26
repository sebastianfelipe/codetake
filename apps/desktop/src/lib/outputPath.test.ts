import { describe, expect, it } from "vitest";
import { dateFolderName, displayPath, recordingFileName, recordingPath } from "./outputPath";

// Months are zero-based in the Date constructor.
const date = new Date(2026, 8, 26, 9, 32, 14);

describe("output path generation", () => {
  it("names folders by date and files by date and time", () => {
    expect(dateFolderName(date)).toBe("2026-09-26");
    expect(recordingFileName(date)).toBe("coding-session-2026-09-26-09-32-14.mp4");
  });

  it("pads single-digit components", () => {
    expect(recordingFileName(new Date(2027, 0, 5, 7, 3, 9))).toBe(
      "coding-session-2027-01-05-07-03-09.mp4",
    );
  });

  it("builds the full path inside a dated folder", () => {
    expect(recordingPath("/Users/dev/Movies/CodeTake", date)).toBe(
      "/Users/dev/Movies/CodeTake/2026-09-26/coding-session-2026-09-26-09-32-14.mp4",
    );
    expect(recordingPath("/Users/dev/Movies/CodeTake/", date)).toBe(
      "/Users/dev/Movies/CodeTake/2026-09-26/coding-session-2026-09-26-09-32-14.mp4",
    );
  });

  it("uses backslashes for Windows directories", () => {
    expect(recordingPath("C:\\Users\\dev\\Videos\\CodeTake", date)).toBe(
      "C:\\Users\\dev\\Videos\\CodeTake\\2026-09-26\\coding-session-2026-09-26-09-32-14.mp4",
    );
  });

  it("abbreviates the home directory", () => {
    expect(displayPath("/Users/dev/Movies/CodeTake", "/Users/dev")).toBe("~/Movies/CodeTake");
    expect(displayPath("/Users/developer/x", "/Users/dev")).toBe("/Users/developer/x");
    expect(displayPath("/Volumes/Ext", null)).toBe("/Volumes/Ext");
  });
});
