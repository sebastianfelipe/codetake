# Background music

CodeTake ships a few loops that can be mixed under a recording.

| File | Title | Author | License |
| --- | --- | --- | --- |
| `coding-01.m4a` | Late Night Commit | CodeTake contributors | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `coding-02.m4a` | Refactor Groove | CodeTake contributors | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `ambient-01.m4a` | Deep Focus | CodeTake contributors | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) |

## Provenance

All three tracks are original works synthesized from scratch by
[`scripts/generate-music.py`](../../scripts/generate-music.py). No samples,
recordings or third-party audio are involved, so there are no upstream
licenses. The tracks are dedicated to the public domain under CC0 1.0:
recordings made with them can be published anywhere, commercially or not,
without attribution.

To regenerate them (macOS, a couple of minutes):

```sh
python3 scripts/generate-music.py assets/music
```

## Adding a track

1. Only add audio you have the right to redistribute under a license that
   allows use in published videos (CC0 or CC BY are ideal). Never add
   copyrighted music downloaded from the internet.
2. Put the file in this folder. AAC (`.m4a`), MP3 and WAV all work; the app
   decodes them to 48 kHz stereo. Short seamless loops work best, because
   the track repeats for the length of the recording.
3. Add an entry to `tracks.json` with its `id`, `title`, `file`, `author`
   and SPDX `license`.
4. Add a row to the table above, with a link to the source and license.
