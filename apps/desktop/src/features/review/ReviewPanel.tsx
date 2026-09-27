import { convertFileSrc } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  type PointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
  type WheelEvent,
} from "react";
import { Hint, Segmented, Select, Slider, Toggle } from "../../components/controls";
import { api, exportVideo, toBackendError } from "../../lib/api";
import { formatDuration } from "../../lib/format";
import type {
  BackendError,
  CameraOverlay,
  MusicTrack,
  RecordingOutcome,
} from "../../types/backend";
import {
  clampOverlay,
  OVERLAY_MAX_SIZE,
  OVERLAY_MIN_SIZE,
  OVERLAY_PRESETS,
  overlayPercentages,
} from "../preview/overlay";
import type { Preset } from "../settings/preset";
import { loopPosition, MAX_RECORDING_VOLUME, musicEnvelope, percent, previewLevels } from "./mix";

type ExportState =
  | { status: "idle" }
  | { status: "exporting"; progress: number }
  | { status: "done"; path: string }
  | { status: "failed"; error: BackendError };

interface Props {
  outcome: RecordingOutcome & { path: string };
  tracks: MusicTrack[];
  music: Preset["music"];
  onMusicChange: (music: Preset["music"]) => void;
  /** Camera layout to start from when the recording has none. */
  defaultOverlay: CameraOverlay;
  revealLabel: string;
  onDone: () => void;
  onRecordAgain: () => void;
}

const PLACEMENTS: { value: keyof typeof OVERLAY_PRESETS; label: string; title: string }[] = [
  { value: "topLeft", label: "↖", title: "Top left" },
  { value: "topRight", label: "↗", title: "Top right" },
  { value: "bottomLeft", label: "↙", title: "Bottom left" },
  { value: "bottomRight", label: "↘", title: "Bottom right" },
];

/** Media files are played through Tauri's asset protocol once allowed. */
function usePlayableUrl(path: string | null): { url: string | null; error: string | null } {
  const [state, setState] = useState<{ url: string | null; error: string | null }>({
    url: null,
    error: null,
  });
  useEffect(() => {
    if (!path) {
      setState({ url: null, error: null });
      return;
    }
    api
      .allowMedia(path)
      .then(() => setState({ url: convertFileSrc(path), error: null }))
      .catch((error) => setState({ url: null, error: toBackendError(error).message }));
  }, [path]);
  return state;
}

/**
 * The last step before the final video: play the raw recording back, place
 * the webcam, add music and balance it against the recording, then save.
 * The raw files are always kept as they are.
 */
export function ReviewPanel({
  outcome,
  tracks,
  music,
  onMusicChange,
  defaultOverlay,
  revealLabel,
  onDone,
  onRecordAgain,
}: Props) {
  const screenRef = useRef<HTMLVideoElement>(null);
  const cameraRef = useRef<HTMLVideoElement>(null);
  const musicRef = useRef<HTMLAudioElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ dx: number; dy: number } | null>(null);

  const screen = usePlayableUrl(outcome.path);
  const camera = usePlayableUrl(outcome.cameraPath);
  const [musicUrl, setMusicUrl] = useState<string | null>(null);
  const [frame, setFrame] = useState({ width: 16, height: 9 });
  const [overlay, setOverlay] = useState<CameraOverlay>(outcome.overlay ?? defaultOverlay);
  const [showCamera, setShowCamera] = useState(outcome.cameraPath !== null);
  const [exportState, setExportState] = useState<ExportState>({ status: "idle" });
  const [confirmLeave, setConfirmLeave] = useState(false);
  const [playback, setPlayback] = useState({ playing: false, time: 0, duration: 0 });

  useEffect(() => {
    if (!music.trackId) {
      setMusicUrl(null);
      return;
    }
    api
      .musicTrackFile(music.trackId)
      .then((path) => setMusicUrl(convertFileSrc(path)))
      .catch(() => setMusicUrl(null));
  }, [music.trackId]);

  /**
   * Volumes and the music fade for the current time. Media elements can't
   * play louder than 100%, so a boosted voice scales both down together:
   * the balance matches the export.
   */
  const applyVolumes = useCallback(() => {
    const video = screenRef.current;
    const audio = musicRef.current;
    if (!video || !audio) return;
    const levels = previewLevels(
      music.recordingVolume,
      music.volume * musicEnvelope(video.currentTime, video.duration),
    );
    video.volume = levels.recording;
    audio.volume = levels.music;
  }, [music.recordingVolume, music.volume]);

  /** Keeps the camera video and the music in step with the screen video. */
  const sync = useCallback(() => {
    const video = screenRef.current;
    if (!video) return;
    const follow = (element: HTMLMediaElement | null, target: number, tolerance: number) => {
      if (!element?.src || !Number.isFinite(element.duration)) return;
      if (Math.abs(element.currentTime - target) > tolerance) element.currentTime = target;
      if (video.paused) element.pause();
      else if (element.paused) void element.play().catch(() => {});
    };
    const cameraVideo = cameraRef.current;
    if (cameraVideo) {
      follow(cameraVideo, Math.min(video.currentTime, cameraVideo.duration || 0), 0.12);
    }
    const audio = musicRef.current;
    if (musicUrl && audio && Number.isFinite(audio.duration)) {
      follow(audio, loopPosition(video.currentTime, audio.duration), 0.25);
    }
  }, [musicUrl]);

  useEffect(applyVolumes, [applyVolumes]);
  useEffect(sync, [sync]);

  const onTimeUpdate = () => {
    applyVolumes();
    sync();
    const video = screenRef.current;
    if (video) {
      setPlayback({
        playing: !video.paused,
        time: video.currentTime,
        duration: Number.isFinite(video.duration) ? video.duration : 0,
      });
    }
  };

  const togglePlay = () => {
    const video = screenRef.current;
    if (!video) return;
    if (video.paused) void video.play().catch(() => {});
    else video.pause();
  };

  const seek = (fraction: number) => {
    const video = screenRef.current;
    if (video && Number.isFinite(video.duration)) video.currentTime = fraction * video.duration;
  };

  /** Pointer position as a fraction of the stage (0..1). */
  const toFraction = (event: { clientX: number; clientY: number }) => {
    const box = stageRef.current?.getBoundingClientRect();
    if (!box || box.width === 0) return null;
    return { x: (event.clientX - box.left) / box.width, y: (event.clientY - box.top) / box.height };
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const point = toFraction(event);
    if (!point) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { dx: point.x - overlay.x, dy: point.y - overlay.y };
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const point = toFraction(event);
    if (!drag.current || !point) return;
    const moved = { ...overlay, x: point.x - drag.current.dx, y: point.y - drag.current.dy };
    setOverlay(clampOverlay(moved, frame.width, frame.height));
  };

  const endDrag = () => {
    drag.current = null;
  };

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    const resized = { ...overlay, size: overlay.size * (event.deltaY < 0 ? 1.05 : 1 / 1.05) };
    setOverlay(clampOverlay(resized, frame.width, frame.height));
  };

  const update = (change: Partial<CameraOverlay>) =>
    setOverlay(clampOverlay({ ...overlay, ...change }, frame.width, frame.height));

  const save = async () => {
    screenRef.current?.pause();
    setConfirmLeave(false);
    setExportState({ status: "exporting", progress: 0 });
    try {
      const path = await exportVideo(
        {
          screen: outcome.path,
          camera: outcome.cameraPath,
          overlay: outcome.cameraPath && showCamera ? overlay : null,
          trackId: music.trackId,
          musicVolume: music.volume,
          recordingVolume: music.recordingVolume,
          destination: outcome.exportPath,
          fps: outcome.fps,
        },
        (progress) => setExportState({ status: "exporting", progress }),
      );
      setExportState({ status: "done", path });
    } catch (error) {
      setExportState({ status: "failed", error: toBackendError(error) });
    }
  };

  const exporting = exportState.status === "exporting";
  const saved = exportState.status === "done";
  const box = overlayPercentages(frame.width, frame.height, overlay);
  const placement =
    PLACEMENTS.find(
      ({ value }) =>
        Math.abs(OVERLAY_PRESETS[value].x - overlay.x) < 0.01 &&
        Math.abs(OVERLAY_PRESETS[value].y - overlay.y) < 0.01,
    )?.value ?? "";

  const leave = () => {
    if (saved || confirmLeave) onDone();
    else setConfirmLeave(true);
  };

  return (
    <div className="review">
      <div className="review-header">
        <p className="recording-label">{saved ? "Saved" : "Review"}</p>
        <span className="muted small">{formatDuration(outcome.durationMs)}</span>
      </div>

      <div
        className="review-stage"
        ref={stageRef}
        style={{
          aspectRatio: `${frame.width} / ${frame.height}`,
          width: `min(100%, calc(44vh * ${frame.width / frame.height}))`,
        }}
      >
        {screen.url ? (
          // biome-ignore lint/a11y/useMediaCaption: screen recordings have no captions
          <video
            ref={screenRef}
            src={screen.url}
            playsInline
            onClick={togglePlay}
            onLoadedMetadata={(e) =>
              setFrame({
                width: e.currentTarget.videoWidth || 16,
                height: e.currentTarget.videoHeight || 9,
              })
            }
            onPlay={onTimeUpdate}
            onPause={onTimeUpdate}
            onSeeked={onTimeUpdate}
            onEnded={onTimeUpdate}
            onTimeUpdate={onTimeUpdate}
          />
        ) : (
          <div className="preview-placeholder">{screen.error ?? "Loading…"}</div>
        )}
        {camera.url && showCamera && (
          <div
            className="review-camera"
            title="Drag to move · scroll to resize"
            style={{
              left: `${box.left}%`,
              top: `${box.top}%`,
              width: `${box.width}%`,
              height: `${box.height}%`,
              borderRadius: box.radius,
            }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onWheel={onWheel}
          >
            <video ref={cameraRef} src={camera.url} muted playsInline onLoadedMetadata={sync} />
          </div>
        )}
        {/* biome-ignore lint/a11y/useMediaCaption: background music, no speech */}
        <audio
          ref={musicRef}
          src={musicUrl ?? undefined}
          loop
          preload="auto"
          onLoadedMetadata={sync}
        />
      </div>

      <div className="player">
        <button
          type="button"
          className="button small"
          onClick={togglePlay}
          disabled={!screen.url}
          aria-label={playback.playing ? "Pause" : "Play"}
        >
          {playback.playing ? "❚❚" : "▶"}
        </button>
        <Slider
          label="Position"
          value={playback.duration ? playback.time / playback.duration : 0}
          onChange={seek}
          disabled={!screen.url}
        />
        <span className="value">
          {formatDuration(playback.time * 1000)} / {formatDuration(playback.duration * 1000)}
        </span>
      </div>

      <fieldset className="review-controls" disabled={exporting}>
        {outcome.cameraPath && (
          <>
            <div className="row labeled">
              <span className="muted">Camera</span>
              <Toggle label="Show" checked={showCamera} onChange={setShowCamera} />
              {showCamera && (
                <>
                  <Segmented
                    label="Camera shape"
                    value={overlay.shape}
                    options={[
                      { value: "circle", label: "Circle" },
                      { value: "square", label: "Square" },
                      { value: "rectangle", label: "Wide" },
                    ]}
                    onChange={(shape) => update({ shape })}
                  />
                  <Segmented
                    label="Camera placement"
                    value={placement}
                    options={PLACEMENTS}
                    onChange={(value) => value && update(OVERLAY_PRESETS[value])}
                  />
                </>
              )}
            </div>
            {showCamera && (
              <div className="row labeled">
                <span className="muted">Size</span>
                <Slider
                  label="Camera size"
                  value={overlay.size}
                  min={OVERLAY_MIN_SIZE}
                  max={OVERLAY_MAX_SIZE}
                  onChange={(size) => update({ size })}
                />
                <span className="value">{percent(overlay.size)}</span>
              </div>
            )}
          </>
        )}
        <div className="row labeled">
          <span className="muted">Music</span>
          <Select
            label="Background music"
            value={music.trackId ?? "none"}
            options={[
              { value: "none", label: "No music" },
              ...tracks.map((t) => ({ value: t.id, label: `${t.title} (${t.license})` })),
            ]}
            onChange={(value) =>
              onMusicChange({ ...music, trackId: value === "none" ? null : value })
            }
          />
        </div>
        {music.trackId && (
          <div className="row labeled">
            <span className="muted">Level</span>
            <Slider
              label="Music volume"
              value={music.volume}
              onChange={(volume) => onMusicChange({ ...music, volume })}
            />
            <span className="value">{percent(music.volume)}</span>
          </div>
        )}
        <div className="row labeled">
          <span className="muted">Voice</span>
          <Slider
            label="Recording volume"
            value={music.recordingVolume}
            max={MAX_RECORDING_VOLUME}
            onChange={(recordingVolume) => onMusicChange({ ...music, recordingVolume })}
          />
          <span className="value">{percent(music.recordingVolume)}</span>
        </div>
        <Hint>
          Press play to check the result
          {outcome.cameraPath ? "; drag the camera to move it and scroll over it to resize" : ""}.
          {music.trackId ? " Music fades in and out and never clips." : ""} What you see and hear
          here is what gets saved.
        </Hint>
      </fieldset>

      {exportState.status === "exporting" && (
        <div
          className="progress"
          role="progressbar"
          aria-valuenow={Math.round(exportState.progress * 100)}
        >
          <div className="progress-fill" style={{ transform: `scaleX(${exportState.progress})` }} />
        </div>
      )}
      {exportState.status === "done" && (
        <p className="hint">
          Saved <span className="path inline">{exportState.path}</span>{" "}
          <button
            type="button"
            className="link"
            onClick={() => void revealItemInDir(exportState.path).catch(() => {})}
          >
            {revealLabel}
          </button>
        </p>
      )}
      {exportState.status === "failed" && <Hint tone="error">{exportState.error.message}</Hint>}
      {confirmLeave && (
        <Hint tone="warning">
          The video hasn't been saved. The raw recording stays in the "raw" folder next to where it
          would be saved. Click Done again to leave anyway.
        </Hint>
      )}
      {outcome.error && <Hint tone="error">{outcome.error.message}</Hint>}
      {outcome.warnings.map((warning) => (
        <Hint key={warning} tone="warning">
          {warning}
        </Hint>
      ))}

      <div className="controls">
        <button type="button" className="button primary" disabled={exporting} onClick={save}>
          {exportState.status === "exporting"
            ? `Saving… ${percent(exportState.progress)}`
            : saved
              ? "Save again"
              : "Save video"}
        </button>
        <button type="button" className="button" disabled={exporting} onClick={onRecordAgain}>
          Record again
        </button>
        <button type="button" className="link" disabled={exporting} onClick={leave}>
          Done
        </button>
      </div>
      <p className="muted small path" title={outcome.exportPath}>
        Saves to {outcome.exportPath}
      </p>
    </div>
  );
}
