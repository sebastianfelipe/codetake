import { homeDir } from "@tauri-apps/api/path";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import logo from "./assets/logo.png";
import { PermissionPanel } from "./components/PermissionPanel";
import { PreviewCanvas } from "./components/PreviewCanvas";
import { canRecord, isBusy } from "./features/recording/machine";
import { RecordingPanel } from "./features/recording/RecordingPanel";
import { ReviewPanel } from "./features/review/ReviewPanel";
import { availableResolutions } from "./features/settings/resolution";
import { SettingsPanel } from "./features/settings/SettingsPanel";
import {
  buildRecordingConfig,
  resolvePreset,
  type SetupContext,
  validateSetup,
} from "./features/settings/validation";
import { buildTrayState, trayElapsed, trayTitle } from "./features/tray/trayState";
import { useBackend } from "./hooks/useBackend";
import { usePreview } from "./hooks/usePreview";
import { useRecording } from "./hooks/useRecording";
import { useSettings } from "./hooks/useSettings";
import { useVisible } from "./hooks/useVisible";
import { api, events, type TrayAction } from "./lib/api";
import { displayPath, recordingPath } from "./lib/outputPath";
import type { PermissionKind } from "./types/backend";

function revealLabel(os: string): string {
  if (os === "macos") return "Show in Finder";
  if (os === "windows") return "Show in Explorer";
  return "Show in folder";
}

export function App() {
  const backend = useBackend();
  const settings = useSettings();
  const [home, setHome] = useState<string | null>(null);
  const [recovered, setRecovered] = useState<string[]>([]);

  useEffect(() => {
    homeDir()
      .then(setHome)
      .catch(() => setHome(null));
  }, []);

  const data = backend.data;
  const preset = useMemo(
    () => (data ? resolvePreset(settings.preset, data) : settings.preset),
    [data, settings.preset],
  );

  const selectedSize = useMemo(() => {
    if (!data || !preset.source) return null;
    const { kind, id } = preset.source;
    const found =
      kind === "display"
        ? data.displays.find((d) => d.id === id)
        : data.windows.find((w) => w.id === id);
    return found ? { width: found.width, height: found.height } : null;
  }, [data, preset.source]);

  const context: SetupContext | null = useMemo(
    () =>
      data
        ? { ...data, availableResolutions: availableResolutions(selectedSize?.height ?? null) }
        : null,
    [data, selectedSize],
  );
  const issues = useMemo(() => (context ? validateSetup(preset, context) : []), [preset, context]);

  const buildConfig = useCallback(
    () => (context ? buildRecordingConfig(preset, context) : null),
    [preset, context],
  );
  const recording = useRecording(buildConfig, preset.countdown);
  const { view } = recording;
  const busy = isBusy(view);
  const visible = useVisible();
  const reviewing = view.phase === "finished" && view.outcome.path !== null;
  const previewEnabled =
    visible &&
    settings.loaded &&
    !!data &&
    !reviewing &&
    (view.phase === "idle" || view.phase === "countdown" || view.phase === "finished");

  const permissionsOk = (kind: PermissionKind) => {
    const status = data?.permissions[kind];
    return status === "granted" || status === "notRequired";
  };
  const preview = usePreview(
    {
      source: permissionsOk("screenRecording") ? preset.source : null,
      cameraId: preset.camera.enabled && permissionsOk("camera") ? preset.camera.deviceId : null,
      microphoneId:
        preset.microphone.enabled && permissionsOk("microphone")
          ? preset.microphone.deviceId
          : null,
    },
    previewEnabled,
  );

  // Recover recordings left behind by a crash, once the output folder is known.
  const outputDirectory = preset.outputDirectory ?? data?.defaultOutputDirectory ?? null;
  const recoveryChecked = useRef(false);
  useEffect(() => {
    if (!outputDirectory || recoveryChecked.current) return;
    recoveryChecked.current = true;
    api
      .recoverRecordings(outputDirectory)
      .then(setRecovered)
      .catch(() => {});
  }, [outputDirectory]);

  // Global shortcut and the menu bar icon drive the same actions.
  const lastSavedPath = view.phase === "finished" ? view.outcome.path : null;
  const actionsRef = useRef<(action: TrayAction | "toggle") => void>(() => {});
  actionsRef.current = (action) => {
    switch (action) {
      case "toggle":
        if (canRecord(view)) {
          if (issues.length === 0) recording.record();
        } else if (view.phase === "recording" || view.phase === "paused") {
          recording.stop();
        } else if (view.phase === "countdown") {
          recording.cancelCountdown();
        }
        break;
      case "record":
        if (canRecord(view) && issues.length === 0) recording.record();
        break;
      case "stop":
        if (view.phase === "recording" || view.phase === "paused") recording.stop();
        break;
      case "pause":
        if (view.phase === "recording") recording.pause();
        break;
      case "resume":
        if (view.phase === "paused") recording.resume();
        break;
      case "cancelCountdown":
        recording.cancelCountdown();
        break;
      case "showFolder": {
        const target = lastSavedPath ?? outputDirectory;
        if (target) void revealItemInDir(target).catch(() => {});
        break;
      }
    }
  };
  useEffect(() => {
    const unlisteners = [
      events.toggleShortcut(() => actionsRef.current("toggle")),
      events.trayAction((action) => actionsRef.current(action)),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
  }, []);

  // Keep the menu bar icon in sync. The menu is only rebuilt when it
  // changes (rebuilding closes an open menu); the title follows the timer.
  const trayState = data ? JSON.stringify(buildTrayState(view, preset, data, issues)) : null;
  useEffect(() => {
    if (trayState) void api.updateTray(JSON.parse(trayState)).catch(() => {});
  }, [trayState]);
  const trayRecording = view.phase === "recording";
  const title = trayTitle(view, preset.menuBarTimer);
  const elapsed = trayElapsed(view);
  useEffect(() => {
    void api.setTrayIndicator(trayRecording, title, elapsed).catch(() => {});
  }, [trayRecording, title, elapsed]);

  if (backend.fatal) {
    return (
      <main className="app centered">
        <p className="hint error">CodeTake could not start: {backend.fatal.message}</p>
      </main>
    );
  }
  if (!data || !context || !settings.loaded) {
    return <main className="app centered muted">Loading…</main>;
  }

  const required: PermissionKind[] = [
    "screenRecording",
    ...(preset.camera.enabled ? (["camera"] as const) : []),
    ...(preset.microphone.enabled ? (["microphone"] as const) : []),
  ];
  const blocking = issues.filter((i) => !i.permission);
  const platformIssue = issues.find((i) => i.field === "platform");
  const cameraOverlay = preset.camera.enabled
    ? {
        shape: preset.camera.shape,
        size: preset.camera.size,
        x: preset.camera.x,
        y: preset.camera.y,
      }
    : null;
  const microphoneLevel = busy ? (recording.status?.microphoneLevel ?? 0) : preview.level;
  const nextPath = outputDirectory ? recordingPath(outputDirectory, new Date()) : null;

  return (
    <main className="app">
      <header className="app-header">
        <img src={logo} alt="" width={32} height={32} />
        <h1>CodeTake</h1>
        <span className="muted small shortcut">
          {data.capabilities.os === "macos" ? "⌘⇧R" : "Ctrl+Shift+R"} to record
        </span>
      </header>

      {platformIssue && <div className="banner error">{platformIssue.message}</div>}
      {recovered.length > 0 && (
        <div className="banner">
          <span>
            Recovered {recovered.length} recording{recovered.length > 1 ? "s" : ""} interrupted by
            an unexpected exit.
          </span>
          <button
            type="button"
            className="link"
            onClick={() => recovered[0] && revealItemInDir(recovered[0])}
          >
            {revealLabel(data.capabilities.os)}
          </button>
          <button type="button" className="link" onClick={() => setRecovered([])}>
            Dismiss
          </button>
        </div>
      )}
      {!platformIssue && (
        <PermissionPanel
          permissions={data.permissions}
          required={required}
          onChange={() => void backend.refresh()}
        />
      )}

      <div className="layout">
        <SettingsPanel
          preset={preset}
          update={settings.update}
          data={data}
          issues={issues}
          sourceHeight={selectedSize?.height ?? null}
          microphoneLevel={microphoneLevel}
          home={home}
          disabled={busy || !!platformIssue}
          onRefreshWindows={() => void backend.refresh()}
        />

        <div className="stage">
          {reviewing && view.phase === "finished" && view.outcome.path ? (
            <ReviewPanel
              outcome={{ ...view.outcome, path: view.outcome.path }}
              tracks={data.tracks}
              music={preset.music}
              defaultOverlay={{
                shape: preset.camera.shape,
                size: preset.camera.size,
                x: preset.camera.x,
                y: preset.camera.y,
              }}
              onMusicChange={(music) => settings.update((p) => ({ ...p, music }))}
              revealLabel={revealLabel(data.capabilities.os)}
              onDone={recording.dismiss}
              onRecordAgain={recording.record}
            />
          ) : (
            <>
              <PreviewCanvas
                screen={preview.screen}
                camera={preview.camera}
                overlay={cameraOverlay}
                aspect={selectedSize ? selectedSize.width / selectedSize.height : 16 / 9}
                onOverlayChange={
                  busy
                    ? undefined
                    : ({ shape, size, x, y }) =>
                        settings.update((p) => ({
                          ...p,
                          camera: { ...p.camera, shape, size, x, y },
                        }))
                }
                placeholder={
                  preview.errors.screen?.message ??
                  (permissionsOk("screenRecording")
                    ? "Preview loading…"
                    : "Grant Screen Recording permission to see a preview")
                }
              />

              {view.phase === "countdown" && (
                <div className="countdown" role="timer" aria-live="assertive">
                  <span key={view.remaining}>{view.remaining}</span>
                  <button type="button" className="link" onClick={recording.cancelCountdown}>
                    Cancel
                  </button>
                </div>
              )}

              {(view.phase === "idle" || view.phase === "countdown") && (
                <div className="record-area">
                  <button
                    type="button"
                    className="record-button"
                    disabled={issues.length > 0 || view.phase === "countdown"}
                    onClick={recording.record}
                  >
                    <span className="dot" aria-hidden="true" /> Record
                  </button>
                  {view.phase === "idle" && view.error && (
                    <p className="hint error">{view.error.message}</p>
                  )}
                  {blocking.length > 0 && issues.length > 0 && (
                    <p className="hint">Fix the highlighted settings to start recording.</p>
                  )}
                  {nextPath && (
                    <p className="muted small path" title={nextPath}>
                      Saves to {displayPath(nextPath, home)}
                    </p>
                  )}
                </div>
              )}

              <RecordingPanel
                view={view}
                status={recording.status}
                microphone={preset.microphone.enabled}
                revealLabel={revealLabel(data.capabilities.os)}
                onPause={recording.pause}
                onResume={recording.resume}
                onStop={recording.stop}
                onDismiss={recording.dismiss}
                onRecordAgain={recording.record}
              />
            </>
          )}
        </div>
      </div>

      <footer className="app-footer muted small">
        Local-first: recordings never leave this computer. No accounts, no telemetry.
      </footer>
    </main>
  );
}
