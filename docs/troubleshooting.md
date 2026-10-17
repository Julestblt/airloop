# Troubleshooting

Logs: Settings → *Open log folder* (`%LOCALAPPDATA%\io.github.julestblt.airloop\logs`).
Include the latest `airloop.*.log` when reporting a problem; logs contain no keys.

**No speakers appear**
- The PC and the HomePod must be on the same network/VLAN; mDNS (UDP 5353)
  must not be blocked. Guest Wi-Fi and "client isolation" break discovery.
- VPN software sometimes captures multicast. Try with the VPN off.

**"The speaker refused the connection"**
- Home app → Home Settings → *Speakers & TV* → *Allow Speaker & TV Access*:
  choose **Everyone** or **Anyone on the Same Network**, and turn off
  *Require Password*.

**Connects, but no sound**
- Check the Airloop volume slider (it is the HomePod's volume) and the level
  meter: if the meter is flat, nothing is being captured. Play something on
  the PC, or choose the right app under *One app*.
- Windows Firewall must allow Airloop on private networks (the installer adds
  the rule; the portable zip does not — accept the firewall prompt, or add an
  inbound rule for `airloop.exe`).
- Make sure the network profile is *Private*, not *Public*.

**Sound cuts out every few seconds / reconnects**
- Weak Wi-Fi on the PC or the HomePod. Wired Ethernet for the PC helps.
- Diagnostics in Settings show buffer underruns and dropped chunks; non-zero
  and growing values point at the PC side.

**"Another app is using the AirPlay clock ports"**
- Only relevant with the experimental PTP mode (`AIRLOOP_TIMING=ptp`). Quit
  other AirPlay/PTP software.

**Windows says the app is from an unknown publisher**
- Builds are not code-signed. *More info* → *Run anyway*, after checking the
  SHA-256 against `SHA256SUMS.txt` from the release page.
