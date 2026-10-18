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

**"Windows Firewall is blocking the speaker…" / stuck on Connecting**
- The HomePod must send timing requests back to the PC (inbound UDP). If
  Windows Firewall blocks them, setup stalls and Airloop reports it.
- Click **Allow in firewall** in the error banner and accept the Windows
  administrator prompt. It removes any block rule for Airloop (created if the
  first-run "Allow access?" prompt was cancelled) and adds an allow rule.
- Or by hand, in an administrator PowerShell:
  `New-NetFirewallRule -DisplayName Airloop -Direction Inbound -Program "C:\Program Files\Airloop\airloop.exe" -Action Allow -Profile Any`

**Connects, but no sound**
- Check the Airloop volume slider (it is the HomePod's volume) and the level
  meter: if the meter is flat, nothing is being captured. Play something on
  the PC, or choose the right app under *One app*.

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
