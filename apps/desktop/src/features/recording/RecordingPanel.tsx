import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { LevelMeter } from "../../components/controls";
import { formatDuration } from "../../lib/format";
import type { RecordingStatus } from "../../types/backend";
import type { RecordingView } from "./machine";

interface Props {
  view: RecordingView;
  status: RecordingStatus | null;
  microphone: boolean;
  /** "Show in Finder" or the platform's equivalent. */
  revealLabel: string;
  onPause: () => void;
  onResume: () => void;
  onStop: () => void;
  onDismiss: () => void;
  onRecordAgain: () => void;
}

/** Timer and controls while recording, and the result afterwards. */
export function RecordingPanel({
  view,
  status,
  microphone,
  revealLabel,
  onPause,
  onResume,
  onStop,
  onDismiss,
  onRecordAgain,
}: Props) {
  if (view.phase === "starting") {
    return (
      <div className="recording-panel">
        <p className="recording-label">Starting…</p>
      </div>
    );
  }

  if (view.phase === "recording" || view.phase === "paused" || view.phase === "stopping") {
    const paused = view.phase === "paused";
    const stopping = view.phase === "stopping";
    return (
      <div className="recording-panel">
        <p className={`recording-label ${paused ? "paused" : stopping ? "" : "live"}`}>
          {stopping ? "Saving…" : paused ? "Paused" : "Recording"}
        </p>
        <p className="timer" aria-live="off">
          {formatDuration(view.elapsedMs)}
        </p>
        {microphone && !stopping && (
          <LevelMeter level={status?.microphoneLevel ?? 0} label="Microphone level" />
        )}
        <div className="controls">
          {paused ? (
            <button type="button" className="button" onClick={onResume} disabled={stopping}>
              Resume
            </button>
          ) : (
            <button type="button" className="button" onClick={onPause} disabled={stopping}>
              Pause
            </button>
          )}
          <button type="button" className="button stop" onClick={onStop} disabled={stopping}>
            ■ Stop
          </button>
        </div>
        {status?.warnings.map((warning) => (
          <p key={warning} className="hint warning">
            {warning}
          </p>
        ))}
        <p className="muted small">CodeTake's own window is not included in the recording.</p>
      </div>
    );
  }

  if (view.phase === "finished") {
    const { outcome } = view;
    return (
      <div className="recording-panel">
        {outcome.path ? (
          <>
            <p className="recording-label">{outcome.complete ? "Saved" : "Partially saved"}</p>
            <p className="timer small">{formatDuration(outcome.durationMs)}</p>
            <p className="path" title={outcome.path}>
              {outcome.path}
            </p>
          </>
        ) : (
          <p className="recording-label">Recording failed</p>
        )}
        {outcome.error && <p className="hint error">{outcome.error.message}</p>}
        {outcome.warnings.map((warning) => (
          <p key={warning} className="hint warning">
            {warning}
          </p>
        ))}
        <div className="controls">
          {outcome.path && (
            <button
              type="button"
              className="button"
              onClick={() => outcome.path && revealItemInDir(outcome.path)}
            >
              {revealLabel}
            </button>
          )}
          <button type="button" className="button primary" onClick={onRecordAgain}>
            Record again
          </button>
          <button type="button" className="link" onClick={onDismiss}>
            Done
          </button>
        </div>
      </div>
    );
  }

  return null;
}
