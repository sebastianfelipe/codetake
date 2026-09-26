# Roadmap

CodeTake 0.1 is deliberately small: configure, record, stop, get an MP4.
Anything below is out of scope until the recorder is rock solid, and each
item needs a design discussion in an issue first.

## Next

- **Windows backend** — Windows.Graphics.Capture, WASAPI (microphone and
  loopback system audio), Media Foundation H.264/AAC.
- **Linux backend** — XDG Desktop Portal ScreenCast, PipeWire audio and
  video, a hardware or software H.264 encoder.
- Signed and notarized macOS releases; `.msi`/`.exe`, `.AppImage` and `.deb`
  once the backends exist.
- Configurable global shortcut.
- Microphone gain and noise reduction.

## Later (ideas, not commitments)

- Presets you can name and switch between.
- HEVC, AV1 or ProRes output.
- Recording a region of the screen.
- Cursor highlighting and click visualization.
- Automatic zoom on activity.
- A simple trim editor working on finished recording sessions.
- Transcription and chapters (on-device only).

## Not planned

- Accounts, cloud storage, uploads, telemetry or analytics. CodeTake is
  local-first; this would only change with explicit opt-in and a very good
  reason.
