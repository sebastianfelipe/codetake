import type { Resolution } from "../../types/backend";

const TARGET_HEIGHT: Record<Resolution, number | null> = {
  source: null,
  "1080p": 1080,
  "1440p": 1440,
  "2160p": 2160,
};

export const RESOLUTION_LABELS: Record<Resolution, string> = {
  source: "Source (native)",
  "1080p": "1080p",
  "1440p": "1440p",
  "2160p": "4K",
};

/**
 * Presets that make sense for a source of the given height. CodeTake never
 * upscales, so presets taller than the source are unavailable (mirrors
 * `resolution_is_available` in src-tauri/src/config.rs).
 */
export function availableResolutions(sourceHeight: number | null): Resolution[] {
  return (Object.keys(TARGET_HEIGHT) as Resolution[]).filter((resolution) => {
    const target = TARGET_HEIGHT[resolution];
    return target === null || (sourceHeight !== null && sourceHeight >= target);
  });
}
