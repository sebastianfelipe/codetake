// Mirrors the music envelope in src-tauri/src/export.rs, so the review
// preview sounds like the exported file.

export const FADE_IN_SECONDS = 1.5;
export const FADE_OUT_SECONDS = 3;
export const MAX_RECORDING_VOLUME = 2;

/** Music gain multiplier (0..1) at `time` seconds into a `duration`-long video. */
export function musicEnvelope(time: number, duration: number): number {
  if (!Number.isFinite(duration) || duration <= 0) return 0;
  const fadeIn = Math.min(1, Math.max(0, time) / FADE_IN_SECONDS);
  const fadeOut = Math.min(1, Math.max(0, duration - time) / FADE_OUT_SECONDS);
  return Math.min(fadeIn, fadeOut);
}

/** Position in a looping track that corresponds to `time` in the video. */
export function loopPosition(time: number, trackDuration: number): number {
  if (!Number.isFinite(trackDuration) || trackDuration <= 0) return 0;
  return ((time % trackDuration) + trackDuration) % trackDuration;
}

export function percent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/**
 * Element volumes (0..1) for previewing a mix. If the recording is boosted
 * above 100%, both are scaled down by the same factor so their balance is
 * the one the export will have.
 */
export function previewLevels(
  recordingGain: number,
  musicGain: number,
): { recording: number; music: number } {
  const scale = Math.max(1, recordingGain, musicGain);
  const clamp = (v: number) => Math.min(1, Math.max(0, v / scale));
  return { recording: clamp(recordingGain), music: clamp(musicGain) };
}
