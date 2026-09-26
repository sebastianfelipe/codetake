// Mirrors the backend's naming (src-tauri/src/output.rs) so the UI can show
// where the next recording will be saved.

const pad = (value: number) => String(value).padStart(2, "0");

export function dateFolderName(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function recordingFileName(date: Date): string {
  const time = `${pad(date.getHours())}-${pad(date.getMinutes())}-${pad(date.getSeconds())}`;
  return `coding-session-${dateFolderName(date)}-${time}.mp4`;
}

function separatorFor(directory: string): string {
  return directory.includes("\\") && !directory.includes("/") ? "\\" : "/";
}

/** The full path a recording started at `date` would be saved to. */
export function recordingPath(directory: string, date: Date): string {
  const separator = separatorFor(directory);
  const base = directory.replace(/[\\/]+$/, "");
  return [base, dateFolderName(date), recordingFileName(date)].join(separator);
}

/** Shortens the home directory to `~` for display. */
export function displayPath(path: string, home: string | null): string {
  if (home) {
    const normalized = home.replace(/[\\/]+$/, "");
    if (path === normalized) {
      return "~";
    }
    if (path.startsWith(`${normalized}/`) || path.startsWith(`${normalized}\\`)) {
      return `~${path.slice(normalized.length)}`;
    }
  }
  return path;
}
