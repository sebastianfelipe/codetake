# Contributing to CodeTake

Thanks for helping! CodeTake aims to be a small, reliable recorder, so the
best contributions make recording more robust or bring it to new platforms.

## Before you start

- For anything bigger than a bug fix, open an issue first to agree on the
  approach. Features outside the current scope (see
  [docs/roadmap.md](docs/roadmap.md)) will likely be declined for now.
- Read [docs/architecture.md](docs/architecture.md). New capture code goes
  behind the traits in `recording/capture.rs`; platform-specific code stays
  in `platform/<os>/`.
- Set up your environment with [docs/development.md](docs/development.md).

## Pull requests

- Keep each pull request focused on one change.
- Add tests for logic you add or change (the recorder, mixer, compositor,
  config and UI state are all unit tested; follow the existing tests).
- Make sure these pass locally — CI runs the same:

  ```sh
  pnpm check && pnpm typecheck && pnpm test
  cd apps/desktop/src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
  ```

- If you change capture or encoding, record a real file with the `record`
  example and check it with `scripts/inspect-recording.swift` (see the
  development guide), and describe what you tested in the pull request.
- Don't claim support for something that isn't implemented: report it
  through `PlatformCapabilities` so the UI can explain it.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): small,
atomic commits that each build and have one purpose.

```
feat(recording): compose screen and camera
fix(macos): handle screen recording permissions
docs: document development workflow
test(recording): add state machine tests
ci: add rust checks
```

Common types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`,
`ci`. Useful scopes: `ui`, `recording`, `audio`, `storage`, `settings`,
`macos`, `windows`, `linux`, `app`, `brand`.

Commit types decide the next release: `fix`/`perf` bump the patch version,
`feat` the minor version, and a breaking change (`feat!:`) the major version
(minor while below 1.0). Releases are prepared automatically; see
[Releases](docs/development.md#releases). Don't change version numbers by hand.

## Code style

- TypeScript: strict mode, no `any`, formatted and linted with Biome.
- Rust: `cargo fmt`, no clippy warnings, meaningful `AppError` variants, no
  `unwrap()` outside tests. Every `unsafe` block gets a `// SAFETY:` comment.

## Assets

Only add music or images you have the right to redistribute under a license
compatible with MIT or CC0, and document it (see
[assets/music/README.md](assets/music/README.md)). Never add copyrighted
music downloaded from the internet.

## Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md).
