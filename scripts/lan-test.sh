#!/usr/bin/env bash
# Runs airloop-cli against real speakers from a Linux box on the same LAN.
#
# PTP needs UDP 319/320, which are privileged on Linux, so the CLI runs as root
# inside a throwaway container on the host network. The container is always
# killed on exit, and the CLI has its own watchdog.
#
#   scripts/lan-test.sh tone "Living Room" --volume 15 --seconds 10
#   scripts/lan-test.sh scan
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
bin="$here/target/release/airloop-cli"
[[ -x "$bin" ]] || { echo "build first: cargo build --release -p airloop-cli" >&2; exit 1; }
name="airloop-lan-test-$$"
log="${AIRLOOP_TEST_LOG:-$here/target/lan-test-$(date +%Y%m%d-%H%M%S).log}"
cleanup() { docker kill "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
echo "log: $log" >&2
docker run --rm --name "$name" --network host \
  -e AIRLOOP_TIMING -e AIRPLAY_NTP_DROP_REQUESTS -e RUST_LOG="${RUST_LOG:-info,airplay_audio=warn,airplay_client=warn,airplay_timing=warn}" \
  -v "$(dirname "$bin"):/w:ro" debian:trixie-slim /w/airloop-cli "$@" 2>&1 | tee "$log"
