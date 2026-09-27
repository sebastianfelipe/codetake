import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { initialView, recordingReducer } from "../features/recording/machine";
import { api, events, toBackendError } from "../lib/api";
import type { RecordingConfig, RecordingStatus } from "../types/backend";

const COUNTDOWN_SECONDS = 3;

/**
 * Drives a recording: countdown, start, pause/resume, stop, and the backend
 * status and completion events.
 */
export function useRecording(buildConfig: () => RecordingConfig | null, countdown: boolean) {
  const [view, dispatch] = useReducer(recordingReducer, initialView);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const configRef = useRef<RecordingConfig | null>(null);
  const buildRef = useRef(buildConfig);
  buildRef.current = buildConfig;

  useEffect(() => {
    const unlisteners = [
      events.status((s) => {
        setStatus(s);
        dispatch({ type: "status", elapsedMs: s.elapsedMs, paused: s.state === "paused" });
      }),
      events.finished((outcome) => dispatch({ type: "finished", outcome })),
    ];
    return () => {
      for (const unlisten of unlisteners) {
        void unlisten.then((fn) => fn());
      }
    };
  }, []);

  // Countdown ticks.
  useEffect(() => {
    if (view.phase !== "countdown") return;
    const timer = window.setTimeout(() => dispatch({ type: "tick" }), 1000);
    return () => window.clearTimeout(timer);
  }, [view]);

  // Start once the countdown is over.
  useEffect(() => {
    if (view.phase !== "starting") return;
    const config = configRef.current;
    if (!config) {
      dispatch({
        type: "startFailed",
        error: { code: "invalid_config", message: "The setup is incomplete.", permission: null },
      });
      return;
    }
    setStatus(null);
    api
      .startRecording(config)
      .then(() => dispatch({ type: "started" }))
      .catch((error) => dispatch({ type: "startFailed", error: toBackendError(error) }));
  }, [view.phase]);

  const record = useCallback(() => {
    configRef.current = buildRef.current();
    if (!configRef.current) return;
    dispatch({ type: "record", countdownSeconds: countdown ? COUNTDOWN_SECONDS : 0 });
  }, [countdown]);

  const pause = useCallback(() => {
    api
      .pauseRecording()
      .then(() => dispatch({ type: "paused" }))
      .catch(() => {});
  }, []);

  const resume = useCallback(() => {
    api
      .resumeRecording()
      .then(() => dispatch({ type: "resumed" }))
      .catch(() => {});
  }, []);

  const stop = useCallback(() => {
    dispatch({ type: "stop" });
    // The outcome also arrives as an event; either delivers the result.
    api
      .stopRecording()
      .then((outcome) => dispatch({ type: "finished", outcome }))
      .catch(() => {});
  }, []);

  const cancelCountdown = useCallback(() => dispatch({ type: "cancelCountdown" }), []);
  const dismiss = useCallback(() => dispatch({ type: "dismiss" }), []);

  return { view, status, record, pause, resume, stop, cancelCountdown, dismiss };
}
