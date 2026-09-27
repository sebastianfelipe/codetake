import { describe, expect, it } from "vitest";
import type { CameraOverlay } from "../../types/backend";
import { clampOverlay, overlayCornerRadius, overlayPercentages, overlayRect } from "./overlay";

const circle: CameraOverlay = { shape: "circle", size: 0.25, x: 0.91, y: 0.84 };

// Expected values match the Rust compositor tests, so the preview and the
// recording agree.
describe("overlay geometry", () => {
  it("places the default circle in the bottom-right corner", () => {
    expect(overlayRect(1920, 1080, circle)).toEqual({ x: 1612, y: 772, width: 270, height: 270 });
  });

  it("centers the overlay on its position", () => {
    expect(overlayRect(1000, 500, { shape: "square", size: 0.2, x: 0.5, y: 0.5 })).toEqual({
      x: 450,
      y: 200,
      width: 100,
      height: 100,
    });
  });

  it("keeps the overlay inside the frame", () => {
    const rect = overlayRect(1000, 500, { ...circle, size: 0.4, x: 0, y: 1 });
    expect(rect).toMatchObject({ x: 0, y: 300 });
  });

  it("makes rectangles landscape with rounded corners", () => {
    const overlay: CameraOverlay = { shape: "rectangle", size: 0.3, x: 0.5, y: 0.5 };
    const rect = overlayRect(1920, 1080, overlay);
    expect(rect.width).toBeGreaterThan(rect.height);
    expect(overlayCornerRadius(rect, overlay)).toBeCloseTo(rect.height * 0.12);
  });

  it("clamps dragged positions and sizes", () => {
    const clamped = clampOverlay({ shape: "square", size: 2, x: -1, y: 5 }, 1000, 500);
    expect(clamped.size).toBe(0.6);
    // A 300 px square in a 1000×500 frame: center at least 150 px from the edges.
    expect(clamped.x).toBeCloseTo(0.15);
    expect(clamped.y).toBeCloseTo(0.7);
  });

  it("expresses the overlay as percentages for positioning over a video", () => {
    const box = overlayPercentages(1000, 500, { shape: "square", size: 0.2, x: 0.5, y: 0.5 });
    expect(box.left).toBeCloseTo(45);
    expect(box.top).toBeCloseTo(40);
    expect(box.width).toBeCloseTo(10);
    expect(box.height).toBeCloseTo(20);
    expect(box.radius).toBe("12% / 12%");
    expect(overlayPercentages(1000, 500, circle).radius).toBe("50%");
  });
});
