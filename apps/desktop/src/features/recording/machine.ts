// The recording flow as seen by the UI:
//
//   idle ─record─▶ countdown ─(3,2,1)─▶ starting ─▶ recording ⇄ paused
//     ▲                │ cancel            │ fail        │ stop
//     │                ▼                   ▼             ▼
//     └──dismiss── finished ◀──────────── stopping ◀─────┘
//
// The backend owns the real recording state; this machine drives what the
// user sees (countdown, timer, "saving…", result) and guards against
// double clicks and invalid actions.

import type { BackendError, RecordingOutcome } from "../../types/backend";

export type RecordingView =
  | { phase: "idle"; error: BackendError | null }
  | { phase: "countdown"; remaining: number }
  | { phase: "starting" }
  | { phase: "recording"; elapsedMs: number }
  | { phase: "paused"; elapsedMs: number }
  | { phase: "stopping"; elapsedMs: number }
  | { phase: "finished"; outcome: RecordingOutcome };

export type RecordingAction =
  | { type: "record"; countdownSeconds: number }
  | { type: "tick" }
  | { type: "cancelCountdown" }
  | { type: "started" }
  | { type: "startFailed"; error: BackendError }
  | { type: "status"; elapsedMs: number; paused: boolean }
  | { type: "paused" }
  | { type: "resumed" }
  | { type: "stop" }
  | { type: "finished"; outcome: RecordingOutcome }
  | { type: "dismiss" };

export const initialView: RecordingView = { phase: "idle", error: null };

function elapsedOf(view: RecordingView): number {
  return "elapsedMs" in view ? view.elapsedMs : 0;
}

export function recordingReducer(view: RecordingView, action: RecordingAction): RecordingView {
  switch (action.type) {
    case "record":
      if (view.phase !== "idle" && view.phase !== "finished") {
        return view;
      }
      return action.countdownSeconds > 0
        ? { phase: "countdown", remaining: action.countdownSeconds }
        : { phase: "starting" };

    case "tick":
      if (view.phase !== "countdown") {
        return view;
      }
      return view.remaining > 1
        ? { phase: "countdown", remaining: view.remaining - 1 }
        : { phase: "starting" };

    case "cancelCountdown":
      return view.phase === "countdown" ? initialView : view;

    case "started":
      return view.phase === "starting" ? { phase: "recording", elapsedMs: 0 } : view;

    case "startFailed":
      return view.phase === "starting" ? { phase: "idle", error: action.error } : view;

    case "status":
      if (view.phase === "recording" || view.phase === "paused") {
        return {
          phase: action.paused ? "paused" : "recording",
          elapsedMs: action.elapsedMs,
        };
      }
      if (view.phase === "stopping") {
        return { ...view, elapsedMs: Math.max(view.elapsedMs, action.elapsedMs) };
      }
      return view;

    case "paused":
      return view.phase === "recording" ? { phase: "paused", elapsedMs: view.elapsedMs } : view;

    case "resumed":
      return view.phase === "paused" ? { phase: "recording", elapsedMs: view.elapsedMs } : view;

    case "stop":
      return view.phase === "recording" || view.phase === "paused"
        ? { phase: "stopping", elapsedMs: elapsedOf(view) }
        : view;

    case "finished":
      // Also accepted while recording: the backend may stop on its own
      // (device lost, disk full) and still deliver a saved file.
      return view.phase === "idle" ? view : { phase: "finished", outcome: action.outcome };

    case "dismiss":
      return view.phase === "finished" ? initialView : view;
  }
}

export function isBusy(view: RecordingView): boolean {
  return view.phase !== "idle" && view.phase !== "finished";
}

export function canRecord(view: RecordingView): boolean {
  return view.phase === "idle" || view.phase === "finished";
}
