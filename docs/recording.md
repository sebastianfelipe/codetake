# Recording details

## Output

- Container: MP4. Video: H.264 High profile, BT.709 color. Audio: AAC-LC,
  48 kHz stereo, 192 kbps (only when a microphone, system audio or music is
  enabled).
- Size: the source's native pixel size, or scaled so the height is 1080,
  1440 or 2160 pixels. The aspect ratio is kept, dimensions are even, and a
  source is never upscaled. Sources larger than the H.264 limit (4096 pixels
  on a side, 36,864 macroblocks — e.g. a 6K display) are scaled down to fit.
- Bitrate: about 0.12 bits per pixel per frame at 30 FPS and 0.09 at 60 FPS,
  between 2 and 80 Mbps (≈ 7.5 Mbps for 1080p30, ≈ 30 Mbps for 4K30). Screen
  recordings of code have sharp text, so this errs on the generous side.
- A keyframe every two seconds, and the index at the start of the file
  ("fast start"), so players and upload sites can seek immediately.

## Files

```
~/Movies/CodeTake/
└── 2026-09-26/
    ├── coding-session-2026-09-26-09-32-14.mp4            finished recording
    ├── coding-session-2026-09-26-11-04-32.partial.mov    being recorded
    ├── coding-session-2026-09-25-18-00-01-recovered.mp4  recovered after a crash
    └── coding-session-2026-09-25-19-12-44-incomplete.mov could not be finalized
```

Two recordings started in the same second get `-2`, `-3`, … suffixes.

## Crash safety

While recording, media is written progressively as a **fragmented
QuickTime movie** (`.partial.mov`), with a new fragment every two seconds.
Nothing is held in memory beyond a few frames and ~200 ms of audio. If
CodeTake is killed or the Mac crashes, the file is playable up to the last
fragment; on the next launch CodeTake finds it, converts it to MP4
(`-recovered.mp4`) and tells you.

When recording stops normally, the movie is **remuxed** to MP4 with a
passthrough export: the H.264 and AAC samples are copied, not re-encoded,
so this is fast and lossless. It needs free space roughly equal to the
recording's size for a moment. If conversion fails, the playable `.mov` is
kept and you are told.

**Why not write fragmented MP4 directly?** We tried: `AVAssetWriter`
intermittently failed to finalize fragmented MPEG-4 files
(error `-11800`, underlying `-16341`) once at least one fragment had been
written — about a third of 4-second test recordings on macOS 26. The same
settings with a QuickTime movie finalized every time, and a non-fragmented
MP4 would be unplayable after a crash.

## Failure handling

| Situation | What happens |
| --- | --- |
| Missing permission | Recording can't start; the UI explains the permission and opens System Settings |
| Camera / microphone unavailable at start | Recording doesn't start; the device error is shown |
| Camera / microphone disconnected while recording | Recording continues (last camera frame / silence); a warning is shown |
| Screen capture stops (display unplugged, window closed) | Recording stops and the file is saved |
| Encoder failure | Recording stops; written media is kept as `-incomplete.mov` |
| Less than 1 GB free when starting | Recording doesn't start |
| Less than 256 MB free while recording | Recording stops cleanly and is saved |
| Invalid or unwritable output folder | Recording doesn't start; the reason is shown |
| Quit (menu bar, ⌘Q, Dock) while recording | The recording is finalized before quitting |
| Crash or forced quit | Recovered on next launch (up to the last two seconds) |

## Known limitations

- macOS only for now (see the README).
- The final ~2 seconds can be lost in a crash (the last fragment).
- The webcam overlay is composited on the CPU. Even at 4K60 this is a
  small amount of work, but it is not free.
- Window capture keeps the window's size at the start of the recording; if
  the window is resized, its content is scaled into that size.
- System audio excludes CodeTake itself, and the preview does not play
  audio.
- The CodeTake window is excluded from display captures. When recording a
  single window, anything covering that window is not captured (by design of
  ScreenCaptureKit window capture).
