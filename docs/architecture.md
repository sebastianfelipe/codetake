# Architecture

CodeTake is a Tauri 2 desktop app. The UI (React + TypeScript) configures and
controls recordings; the Rust core does all capture, mixing, compositing and
encoding. The UI never touches media.

```
apps/desktop/
├── src/                     React UI
│   ├── features/recording/  recording flow state machine and controls
│   ├── features/settings/   presets, validation, settings panel
│   ├── features/preview/    overlay geometry (mirrors the compositor)
│   ├── features/review/     review screen: playback, music, export
│   ├── features/tray/       menu bar state
│   ├── hooks/               backend data, preview, recording, settings
│   ├── lib/                 typed Tauri bindings, formatting, paths
│   └── dev/                 fake backend for browser-only development
└── src-tauri/src/           Rust core
    ├── commands.rs          Tauri commands (the UI's only entry point)
    ├── tray.rs              menu bar icon
    ├── config.rs            RecordingConfig: parsing, validation, sizes
    ├── output.rs            output folders, file names, crash recovery
    ├── music.rs             bundled music catalog
    ├── export.rs            music bed, fades and mixing for exports
    ├── recording/
    │   ├── capture.rs       capture/encoder traits
    │   ├── recorder.rs      the recording pipeline
    │   ├── session.rs       RecordingSession model
    │   ├── state.rs         recording state machine
    │   ├── clock.rs         pause-aware media clock
    │   └── preview.rs       live preview session
    ├── audio/               mixer and sample conversion
    ├── video/compositor.rs  webcam overlay geometry, masks, blending
    └── platform/            OS backends behind the `Platform` trait
        ├── macos/           ScreenCaptureKit, AVFoundation, AudioToolbox
        └── unimplemented.rs Windows/Linux placeholder (reports unsupported)
```

## Recording pipeline

```
ScreenCapture ──▶ latest screen frame ─┐
CameraCapture ──▶ latest camera frame ─┼─▶ video thread (fixed fps) ─┐
                                       │                              ├─▶ VideoEncoder ─▶ file
MicrophoneCapture ─┐                   │                              │
SystemAudioCapture ┼─▶ AudioMixer ◀─ music                            │
                   └─────────────────▶ audio thread (every 20 ms) ────┘
```

- **Timing.** Every capture API on a platform timestamps samples with the
  same monotonic host clock. The `MediaClock` maps host time onto the output
  timeline, starting at zero and removing paused intervals.
- **Video.** A video thread ticks at the chosen frame rate, takes the most
  recent screen and camera frames, copies the screen into an encoder buffer
  and blends the webcam overlay on top. Running at a fixed rate (rather than
  on screen updates) keeps the webcam moving while the screen is static. The
  OS scales the screen capture to the output size.
- **Audio.** Sources are converted to 48 kHz interleaved stereo `f32`, placed
  on the timeline by timestamp, and mixed 200 ms behind real time so late
  buffers still land in the right place. A device that stops delivering
  contributes silence; it can never stall the recording. Music loops from
  the start. The sum is soft-limited below full scale.
- **Supervision.** A supervisor thread owns the captures, handles
  pause/resume/stop, publishes status four times a second, warns when a
  device goes silent, and stops the recording itself — still finalizing the
  file — if a capture or the encoder fails or free disk space drops below
  256 MB.
- **Finalizing.** On stop the threads are joined, remaining audio is
  flushed, the encoder finishes, and the platform converts the intermediate
  file into the final MP4. Written media is never deleted: if something
  fails, the file is kept (as `-incomplete` or in its intermediate format)
  and the user is told where it is.

The pipeline is tested end to end against a fake platform
(`recording/tests.rs`): real threads and timing, synthetic captures, and a
recording encoder.

## Export with music

Music is added after recording, in the review step. `export.rs` holds the
platform-independent parts (looping music bed, fade envelope, mixing with
separate music and voice volumes, soft limiting, file names);
`Platform::export_recording` reads the recording and writes the new file.
On macOS that is `AVAssetReader` + `AVAssetWriter`: video samples are passed
through untouched, audio is decoded to 48 kHz float, mixed and re-encoded
to AAC, and the two tracks are interleaved by timestamp. The review UI's
preview mirrors the same envelope and balance (`features/review/mix.ts`).

## Recording session model

`RecordingSession` captures what was decided when recording started: the
configuration, the source and output sizes, the bitrate, the start time and
the output files. It is kept separate from the running machinery so future
features (for example an editor working on finished sessions) can build on
it without reworking capture.

## The UI

The UI keeps the user's **preset** (persisted as JSON by the backend and
deserialized field by field, so a damaged file degrades to defaults). Before
recording it validates the preset against what is available right now —
platform support, permissions, connected devices — and builds the
`RecordingConfig` sent to the backend, which validates it again.

The recording flow (countdown, starting, recording, paused, saving, result)
is a reducer (`features/recording/machine.ts`) that ignores invalid actions
and accepts stops initiated by the backend. The menu bar icon and the global
shortcut dispatch the same actions as the buttons.

## macOS backend

| Concern | API |
| --- | --- |
| Screen and window capture | ScreenCaptureKit (`SCStream`, BGRA, scaled by the OS, converted to sRGB) |
| System audio | ScreenCaptureKit audio (a separate audio-only stream excluding CodeTake) |
| Camera | AVFoundation `AVCaptureSession` + `AVCaptureVideoDataOutput` |
| Microphone | AVFoundation `AVCaptureAudioDataOutput` (asks for 48 kHz stereo float) |
| Encoding | `AVAssetWriter` → VideoToolbox H.264 (High, BT.709) and AAC |
| MP4 | `AVAssetExportSession` passthrough remux (no re-encoding) |
| Music decoding | AudioToolbox `ExtAudioFile` |
| Permissions | `CGPreflightScreenCaptureAccess`, `AVCaptureDevice` authorization |

Bindings come from the `objc2` family of crates. See
[recording.md](recording.md) for why the encoder writes a QuickTime movie
first.

### Why no FFmpeg?

The OS already provides capture, synchronization, hardware encoding, mixing
input formats and MP4 writing. Using the native APIs keeps the app small,
avoids codec licensing questions, and uses the hardware encoder by default.

## Adding a platform

Implement `platform::Platform` for the new OS (see
`platform/macos/mod.rs` for a complete example):

- `capabilities()` — report honestly what works; the UI disables the rest.
- permissions, device listing and `source_size`.
- `ScreenCapture`, `CameraCapture`, `MicrophoneCapture`,
  `SystemAudioCapture` — deliver BGRA frames and 48 kHz stereo `f32` audio
  (use `audio::convert` to convert) with host-clock timestamps.
- `VideoEncoder` — H.264/AAC written progressively, and
  `finalize_recording` to produce the final MP4.
- `host_time()` — the clock your capture timestamps use.

Suggested APIs: Windows.Graphics.Capture, WASAPI loopback and Media
Foundation on Windows; XDG Desktop Portal (ScreenCast), PipeWire and
GStreamer or VA-API on Linux. Add the platform to the CI build matrix and
update the README's platform table.
