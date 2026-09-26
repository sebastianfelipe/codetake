# Development

## Setup

- macOS 13 or later with the Xcode Command Line Tools (`xcode-select --install`)
- [Rust](https://rustup.rs) stable (with `rustfmt` and `clippy`)
- Node.js 20+ and [pnpm](https://pnpm.io)

```sh
pnpm install
pnpm dev
```

`pnpm dev` starts Vite and the Tauri app with hot reload for the UI; Rust
changes trigger a rebuild. In development macOS attributes permission
prompts to your terminal app, so grant Screen Recording, Camera and
Microphone to the terminal (or run the built `CodeTake.app`).

## Working on the UI in a browser

With `pnpm --filter @codetake/desktop dev` (Vite only) you can open
<http://localhost:1420> in any browser. Outside Tauri the UI installs a fake
backend (`src/dev/mockBackend.ts`) with sample devices, a synthetic preview
and a simulated recording, so layout and flows can be developed without
permissions or capture hardware. It is never included in production builds.

## Checks

| Command | What it runs |
| --- | --- |
| `pnpm check` | Biome formatting and lint (the only JS formatter/linter) |
| `pnpm typecheck` | `tsc` in strict mode |
| `pnpm test` | Vitest unit tests (state machine, presets, validation, paths, geometry, tray) |
| `cargo fmt --check` | Rust formatting |
| `cargo clippy --all-targets -- -D warnings` | Rust lints |
| `cargo test` | Rust tests, including the recorder against a fake platform |

Run the Rust commands in `apps/desktop/src-tauri`. CI runs all of them on
every pull request, with Rust checks on macOS, Linux and Windows.

## Testing capture without the UI

```sh
cd apps/desktop/src-tauri
cargo run --release --example record -- --info            # list displays and devices
cargo run --release --example record -- --seconds 5 --system-audio --out /tmp/rec
cargo run --release --example record -- --seconds 5 --camera --microphone \
  --music ../../../assets/music/coding-01.m4a --out /tmp/rec

swift ../../../scripts/inspect-recording.swift /tmp/rec/*/coding-session-*.mp4 frame.png
```

The example drives the real backend and prints status updates;
`inspect-recording.swift` prints the container, codecs, frame rate and
duration of a file and can save a frame to check the composition.

## Building a release

```sh
pnpm build
```

produces `CodeTake.app` and a `.dmg` in
`apps/desktop/src-tauri/target/release/bundle/`. Signing and notarization
need an Apple Developer ID; see the
[Tauri distribution guide](https://v2.tauri.app/distribute/sign/macos/). The
entitlements needed for the hardened runtime (camera, microphone) are in
`src-tauri/Entitlements.plist`.

The release workflow (`.github/workflows/release.yml`) builds macOS, Windows
and Linux bundles when a `v*` tag is pushed and attaches them to a draft
GitHub Release.

## Assets

- `assets/brand/` — the logo; `scripts/brand-image.swift` crops it and
  generates the menu bar template icon. See its README for the commands.
- `assets/music/` — bundled music; `scripts/generate-music.py` regenerates
  it.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) — see
[CONTRIBUTING.md](../CONTRIBUTING.md).
