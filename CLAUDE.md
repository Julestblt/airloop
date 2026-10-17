# Airloop — guidance for Claude

Windows app streaming PC/app audio to AirPlay 2 speakers (HomePod). Rust engine
+ Tauri 2/React UI. Read `docs/architecture.md` before changing protocol or
audio code.

## Rules

- **Never stream to real speakers at high volume.** Tests use `--volume` ≤ 40
  (UI scale; 40 ≈ −18 dB). The library default is 0 dB = very loud; sessions
  must call `set_initial_volume` before streaming (the engine does).
- Real-speaker tests: `scripts/lan-test.sh` only (container is always killed,
  CLI has a watchdog). Never run a sender with a bare `timeout docker run`:
  killing the client does not stop the container.
- Don't claim something works on Windows unless it ran on Windows. Keep
  `README.md` status table and `docs/limitations.md` truthful.
- NTP timing is the default because it is the verified path; PTP is
  experimental (`AIRLOOP_TIMING=ptp`).

## Layout and commands

- `crates/engine` (portable), `crates/capture` (WASAPI), `crates/cli`,
  `app/src-tauri` (not a default member: needs WebView), `app/src` (UI).
- `cargo test && cargo clippy --all-targets -- -D warnings`
- `cd app && pnpm test && pnpm lint && pnpm typecheck && pnpm format:check`
- Windows cross-check from Linux: `cargo clippy --target x86_64-pc-windows-gnu -p airloop -p airloop-capture`
  (mingw-w64 needed; a docker image recipe lives outside the repo).
- AirPlay stack: `airplay-*` crates from the fork
  `github.com/Julestblt/airplay2-rs` branch `airloop`, pinned by `rev` in
  the root `Cargo.toml`. Changes go to the fork, then bump the rev.

## Conventions

- Conventional commits (`feat:`, `fix:`, `docs:`, `ci:`, `refactor:`, `test:`).
- Squash-merge PRs into `main`; CI must be green.
- Serde types shared with the UI use `camelCase`; mirror them in
  `app/src/lib/types.ts`.
- Errors surface to the UI via `EngineError::user_message()`; never swallow an
  error silently.
