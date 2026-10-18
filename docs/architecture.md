# Architecture

```
 Windows audio ──► airloop-capture ──► FrameSink ──► ElasticBuffer ──► clock thread ──┬─► AirPlay session (speaker 1)
 (WASAPI loopback /   (44.1 kHz f32)     (push)        (drift + gaps)    (10 ms ticks)  └─► AirPlay session (speaker 2)
  process loopback)                                                                       airplay2-rs: RTSP, pairing,
                                                                                          ALAC, ChaCha20, NTP timing
 Tauri shell (app/src-tauri) ── commands/events ──► React UI (app/src)
          │
          └── Engine handle ──► engine actor (crates/engine/src/controller.rs)
```

| Crate | Role | Platform |
|---|---|---|
| `crates/engine` | Discovery, speaker grouping, sessions, clock bridge, state machine | any |
| `crates/capture` | WASAPI system/process loopback, audio app list | Windows (stubs elsewhere) |
| `crates/cli` | Headless sender for protocol tests (`tone`, `capture`, `scan`) | any |
| `app/src-tauri` | Tauri commands, tray, settings, logging, installer | Windows (primary) |
| `app/src` | React + TypeScript + Tailwind UI (the island) | — |

## Key decisions

**Reuse airplay2-rs, via a fork.** Writing an AirPlay 2 sender from scratch
(HomeKit pairing, encrypted RTSP, ALAC, timing) was not justified. We evaluated
[airplay2-rs](https://github.com/lmcgartland/airplay2-rs) and
[OpenAir](https://github.com/Antoinenz/OpenAir) (both GPL-3, both Rust).
airplay2-rs is a library with a usable live-PCM API; OpenAir is more of an
application (TUI) with its own capture. Our fork
([`airloop` branch](https://github.com/Julestblt/airplay2-rs/tree/airloop))
adds:
- Windows portability (socket QoS via `socket2` instead of raw `libc`).
- FDK-AAC made optional: it needs a C++ toolchain and its license is not
  GPL-compatible for binary redistribution. Realtime ALAC does not use it.
- `set_initial_volume`, so a session never starts at the library's 0 dB
  (maximum) default.

**NTP timing, not PTP.** HomePods advertise PTP, and airplay2-rs implements
it (BMCA yield to the HomePod's clock). Against a HomePod mini stereo pair on
audioOS 27.0, one early PTP test was audible, but in a direct A/B test the PTP
session was not heard while the NTP session was, and NTP sessions stayed up
for 90 s+ without dropouts. PTP also spends ~5 s on clock negotiation. NTP also needs no privileged
UDP ports (319/320). `AIRLOOP_TIMING=ptp` re-enables PTP for experiments.

**Stereo pairs are one speaker.** Both members advertise the same `tsid`
(tight-sync id). We address the member whose service name equals the pair name
(`gpn`); the HomePod plays left/right itself.

**A clock bridge between capture and network.** WASAPI delivers bursty audio
on the sound card's clock, and nothing at all while the PC is silent. The
sender is paced by the system clock. `ElasticBuffer` (crates/engine/src/drift.rs)
keeps ~60 ms buffered and adapts the read rate by at most ±0.3 % (cubic
interpolation) to absorb drift, and turns capture gaps into clean silence. A
dedicated clock thread pulls exactly 441 frames every 10 ms and fans them out to
every speaker. Measured drift correction in a 90 s session: < 1 ppm.

**One actor owns the session.** `Engine` is a cheap handle; all state changes
go through a command channel to a single task. Connection attempts run in the
background with a generation counter, so a stale connect can never overwrite a
newer one's audio outputs. States: `idle → connecting → streaming ⇄ paused`,
with `reconnecting` (exponential back-off, up to 6 tries) and `error`.

**Quiet by default.** New installs start at 25 % (≈ −22.5 dB AirPlay). The
volume slider maps linearly to AirPlay's −30…0 dB range, as Apple senders do.

## Protocol notes (verified on hardware, 2026-10)

- HomePod audioOS 27.0 rejects `GET /info` from non-Apple `User-Agent`s with
  403 (see [OwnTone #2042](https://github.com/owntone/owntone-server/issues/2042)).
  airplay2-rs sends `AirPlay/745.83`, which is accepted.
- Transient HomeKit pairing (PIN 3939) is accepted when the Home app allows
  "Everyone" or "Anyone on the same network".
- On 2026-10-16 every third-party sender we tried (airplay2-rs, OpenAir,
  OwnTone) had its RTSP session closed by the HomePod about 10 s after setup.
  The next day the same sessions ran for 90 s+ without being closed; the cause
  of the earlier behaviour is not known. The engine reconnects automatically if
  it happens.

## Security

- No credentials are stored: HomePods use transient pairing. If persistent PIN
  pairing is added, keys must go to Windows Credential Manager.
- Logs never contain keys; pairing material stays inside airplay2-rs.
- The UI has no network access (strict CSP); only Tauri IPC.

## UI: the island

The window is frameless, transparent, always on top, not focusable and
skipped from the taskbar, with a **fixed size** (600×580); it is never resized
while animating, which is what made 0.2.0 stutter. `app/src-tauri/src/island.rs`
runs a watcher thread (30 ms ticks) on the global cursor position:

- hidden, cursor resting ≥150 ms on the top-centre hot zone (±170 px, top
  2 px) → place the window on that monitor, show it, emit `island:reveal`;
- shown → the UI reports the island's bounds (`island_rect`); outside them the
  window ignores the mouse (`set_ignore_cursor_events`), so it never blocks
  clicks;
- cursor away for 450 ms (and not pinned during a drag, not in a timed
  reveal) → emit `island:dismiss`; the UI animates out and calls
  `island_hidden`; the watcher hides the window itself if the UI doesn't
  answer within 1.5 s.

Animations use Motion: one spring for the pill ↔ panel morph (`layout`), the
content cross-fades with `layout="position"` so text is never stretched, and
the island drops in/out from the top edge. Icons are MingCute (Apache-2.0)
compiled per icon by `unplugin-icons`; the Geist fonts are bundled, so the UI
makes no network requests.
