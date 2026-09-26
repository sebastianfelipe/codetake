import { convertFileSrc } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useRef, useState } from "react";
import { Hint, Select, Slider } from "../../components/controls";
import { api, exportRecording, toBackendError } from "../../lib/api";
import { formatDuration } from "../../lib/format";
import type { BackendError, MusicTrack, RecordingOutcome } from "../../types/backend";
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
  revealLabel: string;
  onDone: () => void;
  onRecordAgain: () => void;
}

/**
 * Shown after recording: play the raw recording back, add background music,
 * balance it against the recording, and export a copy with music. The raw
 * recording is always kept as it is.
 */
export function ReviewPanel({
  outcome,
  tracks,
  music,
  onMusicChange,
  revealLabel,
  onDone,
  onRecordAgain,
}: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const [videoUrl, setVideoUrl] = useState<string | null>(null);
  const [musicUrl, setMusicUrl] = useState<string | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [exportState, setExportState] = useState<ExportState>({ status: "idle" });

  useEffect(() => {
    api
      .allowMedia(outcome.path)
      .then(() => setVideoUrl(convertFileSrc(outcome.path)))
      .catch((error) => setMediaError(toBackendError(error).message));
  }, [outcome.path]);

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
   * Applies the volumes (and the music fade) for the current time. Media
   * elements can't play louder than 100%, so when the voice is boosted both
   * sources are scaled down together: the balance matches the export.
   */
  const applyVolumes = useCallback(() => {
    const video = videoRef.current;
    const audio = audioRef.current;
    if (!video || !audio) return;
    const levels = previewLevels(
      music.recordingVolume,
      music.volume * musicEnvelope(video.currentTime, video.duration),
    );
    video.volume = levels.recording;
    audio.volume = levels.music;
  }, [music.recordingVolume, music.volume]);

  const syncMusic = useCallback(() => {
    const video = videoRef.current;
    const audio = audioRef.current;
    if (!video || !audio || !musicUrl || !Number.isFinite(audio.duration)) return;
    const target = loopPosition(video.currentTime, audio.duration);
    if (Math.abs(audio.currentTime - target) > 0.25) {
      audio.currentTime = target;
    }
    if (video.paused) {
      audio.pause();
    } else if (audio.paused) {
      void audio.play().catch(() => {});
    }
  }, [musicUrl]);

  useEffect(applyVolumes, [applyVolumes]);
  useEffect(syncMusic, [syncMusic]);

  const onPlay = () => {
    applyVolumes();
    syncMusic();
  };

  const onTimeUpdate = () => {
    applyVolumes();
    syncMusic();
  };

  const startExport = async () => {
    if (!music.trackId) return;
    videoRef.current?.pause();
    setExportState({ status: "exporting", progress: 0 });
    try {
      const path = await exportRecording(
        {
          source: outcome.path,
          trackId: music.trackId,
          musicVolume: music.volume,
          recordingVolume: music.recordingVolume,
        },
        (progress) => setExportState({ status: "exporting", progress }),
      );
      setExportState({ status: "done", path });
    } catch (error) {
      setExportState({ status: "failed", error: toBackendError(error) });
    }
  };

  const exporting = exportState.status === "exporting";
  const selected = tracks.find((t) => t.id === music.trackId);

  return (
    <div className="review">
      <div className="review-header">
        <p className="recording-label">{outcome.complete ? "Recorded" : "Partially recorded"}</p>
        <span className="muted small">{formatDuration(outcome.durationMs)}</span>
      </div>

      <div className="review-video">
        {videoUrl ? (
          // biome-ignore lint/a11y/useMediaCaption: screen recordings have no captions
          <video
            ref={videoRef}
            src={videoUrl}
            controls
            playsInline
            onPlay={onPlay}
            onPause={syncMusic}
            onSeeked={syncMusic}
            onTimeUpdate={onTimeUpdate}
            onError={() => setMediaError("The recording could not be played here.")}
          />
        ) : (
          <div className="preview-placeholder">{mediaError ?? "Loading…"}</div>
        )}
        {/* biome-ignore lint/a11y/useMediaCaption: background music, no speech */}
        <audio
          ref={audioRef}
          src={musicUrl ?? undefined}
          loop
          preload="auto"
          onLoadedMetadata={syncMusic}
        />
      </div>
      {mediaError && videoUrl && <Hint tone="warning">{mediaError}</Hint>}

      <fieldset className="review-controls" disabled={exporting}>
        <div className="row labeled">
          <span className="muted">Track</span>
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
        <div className="row labeled">
          <span className="muted">Music</span>
          <Slider
            label="Music volume"
            value={music.volume}
            disabled={!music.trackId}
            onChange={(volume) => onMusicChange({ ...music, volume })}
          />
          <span className="value">{percent(music.volume)}</span>
        </div>
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
          {selected
            ? "Press play to hear the mix. Music fades in and out and is softened so it never clips — the export sounds like this preview."
            : "Add music to preview and export a copy with it. Your recording is always kept as it is."}
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
          Exported <span className="path inline">{exportState.path}</span>{" "}
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
      {outcome.error && <Hint tone="error">{outcome.error.message}</Hint>}
      {outcome.warnings.map((warning) => (
        <Hint key={warning} tone="warning">
          {warning}
        </Hint>
      ))}

      <div className="controls">
        <button
          type="button"
          className="button primary"
          disabled={!music.trackId || exporting}
          onClick={startExport}
        >
          {exporting ? `Exporting… ${percent(exportState.progress)}` : "Export with music"}
        </button>
        <button
          type="button"
          className="button"
          disabled={exporting}
          onClick={() => void revealItemInDir(outcome.path).catch(() => {})}
        >
          {revealLabel}
        </button>
        <button type="button" className="button" disabled={exporting} onClick={onRecordAgain}>
          Record again
        </button>
        <button type="button" className="link" disabled={exporting} onClick={onDone}>
          Done
        </button>
      </div>
      <p className="muted small path" title={outcome.path}>
        Original: {outcome.path}
      </p>
    </div>
  );
}
