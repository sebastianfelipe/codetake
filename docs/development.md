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

# composite the raw camera onto the raw screen (circle centered at 70%/35%, 35% tall), with music
cargo run --release --example composite -- /tmp/rec/<day>/raw/<name>-screen.mp4 \
  /tmp/rec/<day>/raw/<name>-camera.mp4 /tmp/final.mp4 circle:0.7:0.35:0.35 ../../../assets/music/coding-01.m4a

# re-mix only the audio of a recording with music (music volume 0.3, voice volume 1.0)
cargo run --release --example export -- /tmp/rec/<day>/raw/<name>-screen.mp4 ../../../assets/music/coding-01.m4a 0.3 1.0
```

The example drives the real backend and prints status updates;
`inspect-recording.swift` prints the container, codecs, frame rate and
duration of a file and can save a frame to check the composition.

## Building a release

```sh
pnpm build:local   # universal .app and .dmg for this Mac, signed with your certificate
pnpm build         # same, for the current architecture, ad-hoc signed
```

**Permissions and signing.** macOS remembers Screen Recording, Camera and
Microphone permissions per code signature. An ad-hoc signature changes with
every build, so a rebuilt, ad-hoc signed app looks like a new app and asks
again even though System Settings still shows CodeTake as allowed.
`pnpm build:local` signs with a certificate from your keychain (a free
*Apple Development* certificate from Xcode → Settings → Accounts is enough),
which keeps the identity stable across builds. If you already granted
permissions to an older build, reset them once and grant them again:

```sh
tccutil reset ScreenCapture dev.codetake.app
tccutil reset Camera dev.codetake.app
tccutil reset Microphone dev.codetake.app
```

produces `CodeTake.app` and a `.dmg` in
`apps/desktop/src-tauri/target/release/bundle/`. By default the bundle is
ad-hoc signed (`signingIdentity: "-"`) with the hardened runtime, which
gives it a stable identity so macOS remembers its permissions on your
machine. Distributing to other Macs without Gatekeeper warnings needs a
Developer ID signature and notarization; see the
[Tauri distribution guide](https://v2.tauri.app/distribute/sign/macos/). The
entitlements needed for the hardened runtime (camera, microphone) are in
`src-tauri/Entitlements.plist`.

## Releases

Releases are automated with
[Release Please](https://github.com/googleapis/release-please) and driven by
the Conventional Commit messages on `main`:

1. Every push to `main` updates an open pull request titled
   **`chore(main): release X.Y.Z`**. It shows the next version before
   anything is published, the `CHANGELOG.md` entry, and the version bumped in
   every file that has one (`apps/desktop/src-tauri/tauri.conf.json`,
   `apps/desktop/package.json`, `apps/desktop/src-tauri/Cargo.toml` and
   `Cargo.lock`).
2. **Merging that pull request releases it**: Release Please creates the
   `vX.Y.Z` tag and a **draft** GitHub Release (with the changelog as release
   notes). The same workflow then builds the installers — a universal macOS
   `.dmg`, Windows `.msi`/`.exe`, and Linux `.AppImage`/`.deb`/`.rpm` —
   uploads them to the draft and **publishes** it. A published release
   therefore always has its installers; while they build (roughly 10–20
   minutes) the release is visible only to maintainers. If a build fails, the
   release stays a draft: fix the problem and re-run **Actions → Release →
   Run workflow** with the tag.

How the next version is chosen (while the version is below 1.0.0):

| Commits since the last release | Next version |
| --- | --- |
| only `fix:` / `perf:` | patch, e.g. 0.1.0 → 0.1.1 |
| at least one `feat:` | minor, e.g. 0.1.0 → 0.2.0 |
| a breaking change (`feat!:` or `BREAKING CHANGE:`) | minor, e.g. 0.1.0 → 0.2.0 (major after 1.0.0) |
| only `docs:`, `chore:`, `ci:`, `test:`, `style:`, `refactor:` | no release pull request |

To force a specific version, add `Release-As: 1.0.0` to the body of a commit
on `main`. Never edit the version numbers by hand: the release pull request
does it, and the build refuses to run if the tag and the app's version
disagree.

To rebuild the installers of an existing release (or retry a failed one),
run **Actions → Release → Run workflow** with its tag.

Release Please runs with the repository's `GITHUB_TOKEN`, which requires
*Settings → Actions → General → Workflow permissions → Allow GitHub Actions to
create and approve pull requests*. Pull requests opened with that token don't
trigger the CI workflow; the release pull request only changes version
numbers and the changelog.

## Assets

- `assets/brand/` — the logo; `scripts/brand-image.swift` crops it and
  generates the menu bar template icon. See its README for the commands.
- `assets/music/` — bundled music; `scripts/generate-music.py` regenerates
  it.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) — see
[CONTRIBUTING.md](../CONTRIBUTING.md).
