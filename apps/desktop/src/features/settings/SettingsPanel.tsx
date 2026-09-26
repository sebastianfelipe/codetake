import { open } from "@tauri-apps/plugin-dialog";
import {
  Hint,
  LevelMeter,
  type Option,
  Section,
  Segmented,
  Select,
  Slider,
  Toggle,
} from "../../components/controls";
import type { BackendData } from "../../hooks/useBackend";
import { formatRefreshRate, formatResolution } from "../../lib/format";
import { displayPath } from "../../lib/outputPath";
import type { Resolution } from "../../types/backend";
import { OVERLAY_MAX_SIZE, OVERLAY_MIN_SIZE, OVERLAY_PRESETS } from "../preview/overlay";
import type { Preset } from "./preset";
import { availableResolutions, RESOLUTION_LABELS } from "./resolution";
import type { Issue, IssueField } from "./validation";

interface Props {
  preset: Preset;
  update: (change: (current: Preset) => Preset) => void;
  data: BackendData;
  issues: Issue[];
  sourceHeight: number | null;
  microphoneLevel: number;
  home: string | null;
  disabled: boolean;
  onRefreshWindows: () => void;
}

const PLACEMENTS: { value: keyof typeof OVERLAY_PRESETS; label: string; title: string }[] = [
  { value: "topLeft", label: "↖", title: "Top left" },
  { value: "topRight", label: "↗", title: "Top right" },
  { value: "bottomLeft", label: "↙", title: "Bottom left" },
  { value: "bottomRight", label: "↘", title: "Bottom right" },
];

/** Which quick placement the overlay is currently at, if any. */
function currentPlacement(x: number, y: number): keyof typeof OVERLAY_PRESETS | "" {
  const match = PLACEMENTS.find(({ value }) => {
    const preset = OVERLAY_PRESETS[value];
    return Math.abs(preset.x - x) < 0.01 && Math.abs(preset.y - y) < 0.01;
  });
  return match?.value ?? "";
}

function sourceValue(preset: Preset): string | null {
  return preset.source ? `${preset.source.kind}:${preset.source.id}` : null;
}

function issueFor(issues: Issue[], field: IssueField) {
  const issue = issues.find((i) => i.field === field && !i.permission);
  return issue ? <Hint tone="error">{issue.message}</Hint> : null;
}

export function SettingsPanel({
  preset,
  update,
  data,
  issues,
  sourceHeight,
  microphoneLevel,
  home,
  disabled,
  onRefreshWindows,
}: Props) {
  const caps = data.capabilities;
  const set = <K extends keyof Preset>(key: K, value: Preset[K]) =>
    update((p) => ({ ...p, [key]: value }));

  const sourceOptions: Option<string>[] = [
    ...data.displays.map((d) => ({
      value: `display:${d.id}`,
      label: [
        d.name,
        formatResolution(d.width, d.height),
        formatRefreshRate(d.refreshRate),
        d.isPrimary ? "Primary" : null,
      ]
        .filter(Boolean)
        .join(" · "),
    })),
    ...(caps.windowCapture.supported
      ? data.windows.map((w) => ({
          value: `window:${w.id}`,
          label: `Window — ${w.appName}: ${w.title}`.slice(0, 90),
        }))
      : []),
  ];

  const resolutions = availableResolutions(sourceHeight);
  const outputDirectory = preset.outputDirectory ?? data.defaultOutputDirectory;

  const chooseDirectory = async () => {
    const chosen = await open({
      directory: true,
      multiple: false,
      defaultPath: outputDirectory ?? undefined,
      title: "Choose where CodeTake saves recordings",
    });
    if (typeof chosen === "string") {
      set("outputDirectory", chosen);
    }
  };

  return (
    <fieldset className="settings" disabled={disabled}>
      <Section
        title="Screen"
        aside={
          caps.windowCapture.supported && (
            <button type="button" className="link" onClick={onRefreshWindows}>
              Refresh
            </button>
          )
        }
      >
        <Select
          label="Screen"
          value={sourceValue(preset)}
          options={sourceOptions}
          placeholder={data.displays.length ? "Choose a screen" : "No screens available"}
          onChange={(value) => {
            const [kind, id] = value.split(":");
            if (kind === "display" || kind === "window") {
              set("source", { kind, id: Number(id) });
            }
          }}
        />
        {issueFor(issues, "source")}
      </Section>

      <Section
        title="Camera"
        aside={
          <Toggle
            label="Enable"
            checked={preset.camera.enabled}
            disabled={!caps.camera.supported}
            onChange={(enabled) => update((p) => ({ ...p, camera: { ...p.camera, enabled } }))}
          />
        }
      >
        {!caps.camera.supported && <Hint tone="warning">{caps.camera.reason}</Hint>}
        {preset.camera.enabled && (
          <>
            <Select
              label="Camera"
              value={preset.camera.deviceId}
              options={data.cameras.map((c) => ({ value: c.id, label: c.name }))}
              placeholder="No cameras found"
              onChange={(deviceId) => update((p) => ({ ...p, camera: { ...p.camera, deviceId } }))}
            />
            <div className="row">
              <Segmented
                label="Camera shape"
                value={preset.camera.shape}
                options={[
                  { value: "circle", label: "Circle" },
                  { value: "square", label: "Square" },
                  { value: "rectangle", label: "Wide" },
                ]}
                onChange={(shape) => update((p) => ({ ...p, camera: { ...p.camera, shape } }))}
              />
              <Segmented
                label="Camera placement"
                value={currentPlacement(preset.camera.x, preset.camera.y)}
                options={PLACEMENTS}
                onChange={(placement) =>
                  placement &&
                  update((p) => ({ ...p, camera: { ...p.camera, ...OVERLAY_PRESETS[placement] } }))
                }
              />
            </div>
            <div className="row labeled">
              <span className="muted">Size</span>
              <Slider
                label="Camera size"
                value={preset.camera.size}
                min={OVERLAY_MIN_SIZE}
                max={OVERLAY_MAX_SIZE}
                onChange={(size) => update((p) => ({ ...p, camera: { ...p.camera, size } }))}
              />
            </div>
            <Hint>Drag the camera in the preview to place it anywhere.</Hint>
            {issueFor(issues, "camera")}
          </>
        )}
      </Section>

      <Section
        title="Microphone"
        aside={
          <Toggle
            label="Enable"
            checked={preset.microphone.enabled}
            disabled={!caps.microphone.supported}
            onChange={(enabled) =>
              update((p) => ({ ...p, microphone: { ...p.microphone, enabled } }))
            }
          />
        }
      >
        {!caps.microphone.supported && <Hint tone="warning">{caps.microphone.reason}</Hint>}
        {preset.microphone.enabled && (
          <>
            <Select
              label="Microphone"
              value={preset.microphone.deviceId}
              options={data.microphones.map((m) => ({ value: m.id, label: m.name }))}
              placeholder="No microphones found"
              onChange={(deviceId) =>
                update((p) => ({ ...p, microphone: { ...p.microphone, deviceId } }))
              }
            />
            <LevelMeter level={microphoneLevel} label="Microphone level" />
            {issueFor(issues, "microphone")}
          </>
        )}
      </Section>

      <Section
        title="System audio"
        aside={
          <Toggle
            label="Record"
            checked={preset.systemAudio}
            disabled={!caps.systemAudio.supported}
            onChange={(value) => set("systemAudio", value)}
          />
        }
      >
        {caps.systemAudio.supported ? (
          <Hint>Sound from other apps, such as a browser or terminal bell.</Hint>
        ) : (
          <Hint tone="warning">{caps.systemAudio.reason}</Hint>
        )}
        {issueFor(issues, "systemAudio")}
      </Section>

      <Section title="Video">
        <div className="row">
          <Select<Resolution>
            label="Resolution"
            value={preset.resolution}
            options={(Object.keys(RESOLUTION_LABELS) as Resolution[]).map((r) => ({
              value: r,
              label: RESOLUTION_LABELS[r],
              disabled: !resolutions.includes(r),
            }))}
            onChange={(resolution) => set("resolution", resolution)}
          />
          <Segmented
            label="Frame rate"
            value={preset.fps}
            options={[
              { value: 30, label: "30 FPS" },
              { value: 60, label: "60 FPS" },
            ]}
            onChange={(fps) => set("fps", fps)}
          />
        </div>
        {issueFor(issues, "resolution")}
      </Section>

      <Section title="Output">
        <div className="row labeled">
          <span className="path" title={outputDirectory ?? undefined}>
            {outputDirectory ? displayPath(outputDirectory, home) : "Not set"}
          </span>
          <button type="button" className="button small" onClick={chooseDirectory}>
            Change…
          </button>
          {preset.outputDirectory && (
            <button type="button" className="link" onClick={() => set("outputDirectory", null)}>
              Default
            </button>
          )}
        </div>
        <Toggle
          label="3-second countdown"
          checked={preset.countdown}
          onChange={(value) => set("countdown", value)}
        />
        {issueFor(issues, "output")}
      </Section>
    </fieldset>
  );
}
