import { useState } from "react";
import { api } from "../lib/api";
import type { PermissionKind, PermissionState, PermissionStatus } from "../types/backend";

const DETAILS: Record<PermissionKind, { title: string; why: string }> = {
  screenRecording: {
    title: "Screen Recording",
    why: "CodeTake needs it to capture your screen and your system audio. Nothing leaves your Mac.",
  },
  camera: {
    title: "Camera",
    why: "CodeTake needs it to show your webcam as an overlay in the recording.",
  },
  microphone: {
    title: "Microphone",
    why: "CodeTake needs it to record your voice with your screen.",
  },
};

function statusText(status: PermissionStatus): string {
  switch (status) {
    case "denied":
      return "Access was denied. Turn it on in System Settings.";
    case "restricted":
      return "Access is blocked by a device policy (for example Screen Time or MDM).";
    default:
      return "Not granted yet.";
  }
}

interface Props {
  permissions: PermissionState;
  required: PermissionKind[];
  onChange: () => void;
}

/** Explains each missing permission and offers a way to grant it. */
export function PermissionPanel({ permissions, required, onChange }: Props) {
  const [asked, setAsked] = useState<Partial<Record<PermissionKind, boolean>>>({});
  const missing = required.filter(
    (kind) => permissions[kind] !== "granted" && permissions[kind] !== "notRequired",
  );
  if (missing.length === 0) {
    return null;
  }

  const request = async (kind: PermissionKind) => {
    await api.requestPermission(kind).catch(() => undefined);
    setAsked((a) => ({ ...a, [kind]: true }));
    onChange();
  };

  return (
    <div className="permissions" role="alert">
      <h2>CodeTake needs your permission</h2>
      {missing.map((kind) => {
        const status = permissions[kind];
        const canPrompt = status === "notDetermined" && !asked[kind];
        return (
          <div key={kind} className="permission">
            <div>
              <strong>{DETAILS[kind].title}</strong>
              <p>{DETAILS[kind].why}</p>
              <p className="muted">
                {statusText(status)}
                {kind === "screenRecording" &&
                  " After turning it on, quit and reopen CodeTake so macOS applies it."}
              </p>
            </div>
            <div className="permission-actions">
              {canPrompt && (
                <button type="button" className="button primary" onClick={() => request(kind)}>
                  Allow access
                </button>
              )}
              <button
                type="button"
                className="button"
                onClick={() => api.openPermissionSettings(kind).catch(() => undefined)}
              >
                Open System Settings
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
