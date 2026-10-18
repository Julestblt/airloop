# Airloop

Stream your Windows PC's audio — or one app's audio — to HomePod and other
AirPlay 2 speakers.

Launch it, pick a speaker (a Home-app stereo pair shows up as one speaker),
choose **Whole PC** or **One app**, press **Connect**.

> Airloop is an independent open-source project, not affiliated with Apple.

## Status

**Early release (0.1).** What has and has not been verified, honestly:

| Capability | Status |
|---|---|
| Discovery of AirPlay 2 speakers (mDNS) | ✅ Verified on a real network |
| HomePod mini stereo pair shown as one speaker | ✅ Verified (audioOS 27.0) |
| Streaming to a HomePod mini stereo pair | ✅ Verified audible, 90 s session without dropouts (test tone, from Linux) |
| Volume control | ✅ Verified (AirPlay −30…0 dB scale) |
| Whole-PC capture (WASAPI loopback) | ⚠️ Implemented, compiles and is unit-tested; **not yet run on real Windows hardware** |
| One-app capture (process loopback) | ⚠️ Implemented (Windows 11 / Win10 build 20348+); **not yet run on real hardware** |
| Several speakers at once | ⚠️ Implemented (shared NTP clock + SETPEERS); not verified — no independent speakers were available |
| Single unpaired HomePod, Apple TV, third-party AirPlay 2 | ❓ Expected to work, untested |
| Browser-tab capture | ❌ Not possible without a browser extension; capture the browser app instead |
| Password-protected / PIN-paired receivers | ❌ Not supported yet (shown as unsupported) |
| Code signing | ❌ Builds are unsigned (SmartScreen warns) |

See [docs/limitations.md](docs/limitations.md) for details.

## Install

Download `Airloop_x.y.z_x64-setup.exe` from the
[Releases](https://github.com/Julestblt/airloop/releases) page and run it.

- The installer asks for administrator rights to add one Windows Firewall
  rule (inbound traffic for Airloop only). AirPlay speakers send timing and
  retransmit requests back to the sender; without the rule the connection
  stalls. If it is ever missing, the app offers **Allow in firewall**.
- The build is **not code-signed**: SmartScreen shows "Windows protected your
  PC" → *More info* → *Run anyway*. Compare the file with `SHA256SUMS.txt`.
- Requires Windows 10 or 11 (x64) with the Edge WebView2 runtime (preinstalled
  on Windows 11; the installer fetches it if missing).

A portable `.zip` (no installer, no firewall rule) is also published.

## Using it

Airloop lives in a black **island** at the top of the screen, inspired by the
iPhone's Dynamic Island. It stays **hidden** until you want it:

- **Reveal**: push the mouse against the top edge of the screen, in the middle,
  for a moment. The island drops down as a **pill** (speaker, live level bars).
- **Open**: rest the mouse on the pill (or click it) to open the **panel**:
  choose **Whole PC** or an app, pick one or more speakers (a Home-app stereo
  pair is one entry), set the volume, press **Play**. While playing:
  **Pause** / **Disconnect**.
- **Leave**: move the mouse away and it folds back up and disappears. It also
  pops down for a few seconds by itself when playback starts or fails.
- It never takes keyboard focus, and the space around it lets clicks through.
- The tray icon opens the island, reconnects to the last speakers, or quits.

Notes:

- **Volume** is the speaker's own volume (AirPlay volume), independent of the
  Windows volume. It starts at a quiet 25 % and remembers the last value.
- **Your PC's speakers keep playing** while capturing the whole PC; mute them
  or pick an output with nothing attached if you only want the HomePod.
- Expect about **2 seconds of delay**: AirPlay buffers audio on the speaker.
  Fine for music and podcasts, not for games or video calls.
- Device icons: [MingCute](https://github.com/Richard9394/MingCute) (Apache-2.0).

## Documentation

- [Architecture and decisions](docs/architecture.md)
- [Development, testing and releases](docs/development.md)
- [Known limitations](docs/limitations.md)
- [Troubleshooting](docs/troubleshooting.md)

## License

GPL-3.0-or-later. The AirPlay protocol stack comes from
[airplay2-rs](https://github.com/lmcgartland/airplay2-rs) (GPL-3.0-or-later),
used through [a fork](https://github.com/Julestblt/airplay2-rs/tree/airloop)
with Windows portability fixes.
