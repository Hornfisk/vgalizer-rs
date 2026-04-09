#!/usr/bin/env bash
# beat-test-arch.sh — quick local beat-tracker sanity run on Arch.
#
# Usage:  scripts/beat-test-arch.sh [seconds] [audio_device_substring]
#   defaults: 30 seconds, "C920" (webcam mic — point a metronome at it)
#
# Common picks for the second arg on this box:
#   C920       → webcam mic (play metronome out loud)
#   D500       → DJControl Inpulse 500 line-in (play a DJ track through it)
#   Generic_1  → onboard analog line-in
#   pipewire   → default PipeWire route
#
# After the run it tails the last ~15 beat-dbg lines so you can read off
# the locked BPM. Pass criterion on a 150 BPM metronome: locked ≈ 150.0.

set -u

DURATION="${1:-30}"
DEV="${2:-C920}"

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$REPO/target/release/vgalizer"

if [ ! -x "$BIN" ]; then
    echo "error: $BIN not built — cargo build --release first" >&2
    exit 2
fi

STAMP=$(date +%Y%m%d-%H%M%S)
LOG="/tmp/beat-test-arch-${STAMP}.log"

echo "device:   $DEV"
echo "duration: ${DURATION}s"
echo "log:      $LOG"
echo

RUST_LOG="vgalizer::audio::beat=debug,warn" \
    timeout "${DURATION}s" \
    "$BIN" --windowed --resolution 800x600 --audio-device "$DEV" \
    >"$LOG" 2>&1 || true

echo "=== last 15 beat-dbg lines ==="
grep "beat-dbg:" "$LOG" | tail -15
echo
echo "=== lock summary ==="
LOCKED=$(grep "beat-dbg:" "$LOG" | grep "locked=true" | tail -1)
if [ -n "$LOCKED" ]; then
    echo "$LOCKED" | grep -oE "bpm=[0-9.]+ locked=true ri_len=[0-9]+ ri_mean=[0-9.]+s ri_stddev=[0-9.]+ms"
else
    echo "never locked — check ri_stddev on the last unlocked line:"
    grep "beat-dbg:" "$LOG" | tail -1
fi
