import { describe, expect, it } from "vitest";
import type { CameraOverlay } from "../../types/backend";
import { overlayCornerRadius, overlayRect } from "./overlay";

const circle: CameraOverlay = { size: "medium", position: "bottomRight", shape: "circle" };

// Expected values match the Rust compositor tests, so the preview and the
// recording agree.
describe("overlay geometry", () => {
  it("places a medium circle in the bottom-right corner by default", () => {
    expect(overlayRect(1920, 1080, circle)).toEqual({ x: 1618, y: 778, width: 270, height: 270 });
  });

  it("maps positions to corners", () => {
    const at = (position: CameraOverlay["position"]) =>
      overlayRect(1000, 500, { ...circle, position });
    expect(at("topLeft")).toMatchObject({ x: 15, y: 15 });
    const topRight = at("topRight");
    expect(topRight.x + topRight.width).toBe(985);
    const bottomLeft = at("bottomLeft");
    expect(bottomLeft.x).toBe(15);
    expect(bottomLeft.y + bottomLeft.height).toBe(485);
  });

  it("makes rounded rectangles landscape", () => {
    const overlay: CameraOverlay = { ...circle, shape: "roundedRectangle" };
    const rect = overlayRect(1920, 1080, overlay);
    expect(rect.width).toBeGreaterThan(rect.height);
    expect(overlayCornerRadius(rect, overlay)).toBeCloseTo(rect.height * 0.16);
  });

  it("scales with the size setting", () => {
    const small = overlayRect(1920, 1080, { ...circle, size: "small" });
    const large = overlayRect(1920, 1080, { ...circle, size: "large" });
    expect(small.height).toBeLessThan(large.height);
  });
});
