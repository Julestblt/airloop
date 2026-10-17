# Development

## Prerequisites

- Rust stable (see `rust-version` in `Cargo.toml`).
- Node 22 and pnpm (version pinned in `app/package.json` → `packageManager`).
- On Windows: Visual Studio Build Tools ("Desktop development with C++") and
  WebView2 (preinstalled on Windows 11).

## Layout

```
crates/engine     platform-independent engine (discovery, sessions, clock bridge)
crates/capture    WASAPI capture (Windows), stubs elsewhere
crates/cli        airloop-cli: headless sender for tests
app/              Tauri 2 app: React UI in src/, Rust shell in src-tauri/
scripts/          lan-test.sh (real-speaker tests from Linux)
```

`app/src-tauri` is a workspace member but not a default member, so plain
`cargo build`/`cargo test` on Linux does not need WebKitGTK.

## Common commands

```bash
cargo test                          # engine, capture, cli
cargo clippy --all-targets -- -D warnings
cd app && pnpm install && pnpm test # UI unit tests (mock backend)
cd app && pnpm dev                  # UI in a browser, mock backend
cd app && pnpm tauri dev            # full app (Windows)
cd app && pnpm tauri build --bundles nsis   # installer → target/release/bundle/nsis/
```

## Testing strategy

| Layer | How | Where |
|---|---|---|
| Engine logic (grouping, volume, clock bridge, drift, errors) | Unit tests, incl. a 10-minute simulated 200 ppm drift | `cargo test`, CI Linux + Windows |
| UI | Vitest + Testing Library against the mock backend | CI |
| Windows code (capture, Tauri shell) | `cargo clippy`/`cargo test` on `windows-latest`, installer packaging | CI |
| Protocol / real speakers | `airloop-cli tone` against real HomePods from a Linux host | manual, `scripts/lan-test.sh` |
| Windows capture on hardware | Manual: install, play music, connect | **pending** |

### Real-speaker tests from Linux

```bash
cargo build --release -p airloop-cli
scripts/lan-test.sh scan --seconds 5
scripts/lan-test.sh tone "Living Room" --volume 30 --seconds 20 --freq 300
```

The script runs the CLI as root in a throwaway container on the host network
(needed only for `AIRLOOP_TIMING=ptp`, which binds UDP 319/320), always kills
it on exit, and logs to `target/lan-test-*.log`. The CLI has its own watchdog.
**Keep test volumes low**: a HomePod at 0 dB is very loud.

Cross-checking Windows code from Linux works with the `x86_64-pc-windows-gnu`
target and mingw-w64 (`cargo clippy --target x86_64-pc-windows-gnu -p airloop`).

## Releases

1. Bump `version` in `Cargo.toml`, `app/package.json` and
   `app/src-tauri/tauri.conf.json`.
2. Tag: `git tag v0.1.1 && git push origin v0.1.1`.
3. `.github/workflows/release.yml` builds on `windows-latest` and publishes the
   NSIS installer, a portable zip and `SHA256SUMS.txt` to a GitHub release.

Builds are unsigned. Reproducing a release: check out the tag and run the same
commands as the workflow (`pnpm install --frozen-lockfile`,
`pnpm tauri build --bundles nsis`).
