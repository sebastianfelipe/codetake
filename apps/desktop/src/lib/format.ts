/** Formats a duration as HH:MM:SS. */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return [hours, minutes, seconds].map((v) => String(v).padStart(2, "0")).join(":");
}

export function formatResolution(width: number, height: number): string {
  return `${width} × ${height}`;
}

export function formatRefreshRate(hz: number | null): string | null {
  return hz ? `${Math.round(hz)} Hz` : null;
}
