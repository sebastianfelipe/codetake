import { describe, expect, it } from "vitest";
import type { RecordingOutcome } from "../../types/backend";
import {
  canRecord,
  initialView,
  isBusy,
  type RecordingAction,
  type RecordingView,
  recordingReducer,
} from "./machine";

function run(actions: RecordingAction[], from: RecordingView = initialView): RecordingView {
  return actions.reduce(recordingReducer, from);
}

const outcome: RecordingOutcome = {
  path: "/Movies/CodeTake/2026-09-26/coding-session.mp4",
  durationMs: 5000,
  complete: true,
  error: null,
  warnings: [],
};

const error = { code: "screen_capture", message: "no frames", permission: null };

describe("recording state machine", () => {
  it("counts down 3, 2, 1 before starting", () => {
    let view = run([{ type: "record", countdownSeconds: 3 }]);
    expect(view).toEqual({ phase: "countdown", remaining: 3 });
    view = run([{ type: "tick" }], view);
    expect(view).toEqual({ phase: "countdown", remaining: 2 });
    view = run([{ type: "tick" }, { type: "tick" }], view);
    expect(view).toEqual({ phase: "starting" });
  });

  it("skips the countdown when it is disabled", () => {
    expect(run([{ type: "record", countdownSeconds: 0 }])).toEqual({ phase: "starting" });
  });

  it("can cancel the countdown", () => {
    expect(run([{ type: "record", countdownSeconds: 3 }, { type: "cancelCountdown" }])).toEqual(
      initialView,
    );
  });

  it("goes through recording, pause, resume, stop and finish", () => {
    const view = run([
      { type: "record", countdownSeconds: 0 },
      { type: "started" },
      { type: "status", elapsedMs: 1200, paused: false },
      { type: "paused" },
      { type: "resumed" },
      { type: "status", elapsedMs: 4000, paused: false },
      { type: "stop" },
    ]);
    expect(view).toEqual({ phase: "stopping", elapsedMs: 4000 });
    expect(run([{ type: "finished", outcome }], view)).toEqual({ phase: "finished", outcome });
  });

  it("follows the backend's paused state from status updates", () => {
    const view = run([
      { type: "record", countdownSeconds: 0 },
      { type: "started" },
      { type: "status", elapsedMs: 900, paused: true },
    ]);
    expect(view).toEqual({ phase: "paused", elapsedMs: 900 });
  });

  it("returns to idle with the error when starting fails", () => {
    const view = run([
      { type: "record", countdownSeconds: 0 },
      { type: "startFailed", error },
    ]);
    expect(view).toEqual({ phase: "idle", error });
  });

  it("shows the result when the backend stops on its own", () => {
    const view = run([
      { type: "record", countdownSeconds: 0 },
      { type: "started" },
      { type: "finished", outcome: { ...outcome, error } },
    ]);
    expect(view.phase).toBe("finished");
  });

  it("ignores invalid actions", () => {
    expect(run([{ type: "stop" }])).toEqual(initialView);
    expect(run([{ type: "paused" }])).toEqual(initialView);
    expect(run([{ type: "finished", outcome }])).toEqual(initialView);
    const recording = run([{ type: "record", countdownSeconds: 0 }, { type: "started" }]);
    // A second click on Record while recording does nothing.
    expect(run([{ type: "record", countdownSeconds: 3 }], recording)).toBe(recording);
    expect(run([{ type: "resumed" }], recording)).toBe(recording);
  });

  it("allows recording again after dismissing or directly from the result", () => {
    const finished = run([
      { type: "record", countdownSeconds: 0 },
      { type: "started" },
      { type: "stop" },
      { type: "finished", outcome },
    ]);
    expect(canRecord(finished)).toBe(true);
    expect(run([{ type: "dismiss" }], finished)).toEqual(initialView);
    expect(run([{ type: "record", countdownSeconds: 3 }], finished).phase).toBe("countdown");
  });

  it("reports busy phases", () => {
    expect(isBusy(initialView)).toBe(false);
    expect(isBusy({ phase: "countdown", remaining: 2 })).toBe(true);
    expect(isBusy({ phase: "stopping", elapsedMs: 0 })).toBe(true);
    expect(isBusy({ phase: "finished", outcome })).toBe(false);
  });
});
