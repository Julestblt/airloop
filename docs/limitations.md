# Known limitations

- **Windows capture: partly validated.** Capturing one app (Brave) to a
  HomePod mini stereo pair was confirmed working on a real Windows PC with
  0.1.2. Whole-PC capture runs the same pipeline but has not been confirmed by
  a user yet.
- **Latency is about 2 seconds.** Realtime AirPlay buffers on the receiver
  (~2 s). Airloop itself adds ~0.2 s since 0.2.2 (it used to add ~1 s). An
  experimental *Low latency* setting asks the speaker for a shorter buffer;
  whether a given speaker honours it can only be judged by ear. Video players
  that can shift audio (VLC: J/K keys) can compensate; browsers cannot.
- **PC speakers keep playing** during whole-PC capture. Loopback capture taps
  what Windows sends to the output device, so that device still plays it.
- **Browser-tab capture is not available.** Windows can only isolate audio per
  process. Chrome/Edge/Firefox play all tabs through one audio process, so
  "One app" captures the whole browser. Per-tab capture would need a browser
  extension (`chrome.tabCapture`) plus native messaging. Out of scope for now.
- **One-app capture needs Windows 11** (or Windows 10 build 20348+), which
  added process loopback. On older Windows the option is disabled.
- **Protected audio**: content that Windows marks as protected may be silenced
  in loopback capture by the OS.
- **Several independent speakers**: implemented (shared NTP clock, SETPEERS),
  but only verified with a single stereo pair. Sync quality between
  independent speakers is unmeasured.
- **Password/PIN-protected receivers** (Apple TV with "Require password",
  speakers with access limited to Home members) are not supported. The UI marks
  them as unsupported.
- **Session drops**: on one test day the HomePod closed every third-party
  session after ~10 s (see architecture notes). The engine reconnects
  automatically (up to 6 tries), which would be audible as gaps.
- **If someone else AirPlays to the speaker** (an iPhone, say), the HomePod
  ends our session. With auto-reconnect on, Airloop retries and may take the
  speaker back. Turn auto-reconnect off in Settings if that's a problem.
- **Unsigned builds**: SmartScreen warns on first run.
- **No macOS/Linux app**: the engine is portable, but capture and packaging are
  Windows-only today.
