import { describe, expect, it } from "vitest";
import { availableResolutions } from "./resolution";

describe("available resolutions", () => {
  it("offers every preset up to the source height", () => {
    expect(availableResolutions(2160)).toEqual(["source", "1080p", "1440p", "2160p"]);
    expect(availableResolutions(1800)).toEqual(["source", "1080p", "1440p"]);
  });

  it("never offers upscaling", () => {
    expect(availableResolutions(1080)).toEqual(["source", "1080p"]);
    expect(availableResolutions(900)).toEqual(["source"]);
  });

  it("only offers source size when the source is unknown", () => {
    expect(availableResolutions(null)).toEqual(["source"]);
  });
});
