#!/usr/bin/env bash
# vjtest.sh — monitored, bounded vgalizer soak run
#
# Usage:  vjtest.sh <seconds>              → headless mode, SSH-safe
#         vjtest.sh <seconds> headless     → same (explicit)
#         vjtest.sh <seconds> windowed     → --windowed 1280x720, needs a display
#         vjtest.sh <seconds> cage         → cage kiosk on the physical tty
#
# Captures stdout, stderr, periodic RSS snapshots, and a post-run summary
# with peak RSS, fps percentiles from the `perf:` log lines, beat-lock
# count, and any warnings/errors. ^C is safe — the trap still writes the
# summary.
#
# Logs land in ~/vgalizer-logs/<mode>-<YYYYmmdd-HHMMSS>/ and the summary
# is printed to the terminal on exit.

set -u

REPO="/home/natalia/repos/vgalizer-rs"
BIN="$REPO/target/release/vgalizer"

DURATION="${1:-60}"
MODE="${2:-headless}"

if [ ! -x "$BIN" ]; then
    echo "error: $BIN not built — run 'cargo build --release' first" >&2
    exit 2
fi

case "$DURATION" in
    ''|*[!0-9]*) echo "error: duration must be an integer (seconds)" >&2; exit 2 ;;
esac

STAMP=$(date +%Y%m%d-%H%M%S)
LOGDIR="$HOME/vgalizer-logs/${MODE}-${STAMP}"
mkdir -p "$LOGDIR"

# Sample RSS more often on short runs, less often on long ones.
if [ "$DURATION" -le 120 ]; then
    RSS_INTERVAL=2
else
    RSS_INTERVAL=10
fi

echo "vjtest: mode=$MODE duration=${DURATION}s rss_interval=${RSS_INTERVAL}s"
echo "vjtest: logs -> $LOGDIR"
echo "vjtest: binary $(stat -c %y "$BIN")"
echo

# --- Launch vgalizer ---------------------------------------------------------
#
# Three modes:
#
#   headless — WLR_BACKENDS=headless cage -- vgalizer. Runs from SSH
#              with no physical display. cage provides a wlroots
#              compositor backed by a virtual framebuffer, vgalizer
#              renders normally into it, and nothing is presented to
#              real hardware. Exercises the full effect + post + blit
#              pipeline so RSS, perf, and beat lock all measure as on
#              real hardware. Same trick documented in
#              memory/feedback_cage_spawn_diagnosis.md.
#
#   windowed — native `vgalizer --windowed 1280x720`. Requires an
#              existing WAYLAND_DISPLAY or DISPLAY (i.e. must be run
#              from inside a compositor session, not raw SSH).
#
#   cage     — real cage kiosk on the physical tty, fullscreen. The
#              production path. Must be launched from the tty, not
#              over SSH.
cd "$REPO" || exit 3
case "$MODE" in
    headless)
        RUST_LOG=info WLR_BACKENDS=headless cage -- bash -c \
            "cd $REPO && exec ./target/release/vgalizer" \
            >"$LOGDIR/stdout.log" 2>"$LOGDIR/stderr.log" &
        ;;
    cage)
        RUST_LOG=info cage -- bash -c \
            "cd $REPO && exec ./target/release/vgalizer" \
            >"$LOGDIR/stdout.log" 2>"$LOGDIR/stderr.log" &
        ;;
    windowed)
        RUST_LOG=info "$BIN" --windowed -r 1280x720 \
            >"$LOGDIR/stdout.log" 2>"$LOGDIR/stderr.log" &
        ;;
    *)
        echo "error: unknown mode '$MODE' (expected headless|windowed|cage)" >&2
        exit 2
        ;;
esac
VJPID=$!

# Small grace period for the process to actually spawn. If it dies
# immediately, bail with the log tail so the user doesn't wait.
sleep 1
if ! kill -0 "$VJPID" 2>/dev/null; then
    echo "vjtest: vgalizer died immediately. stderr tail:"
    tail -20 "$LOGDIR/stderr.log"
    exit 4
fi

# Under cage the direct $VJPID is cage itself; find the vgalizer child.
# `pgrep -n vgalizer` = most recently spawned. Good enough since soak
# runs don't overlap.
VPID=$(pgrep -n -x vgalizer 2>/dev/null || echo "$VJPID")
echo "vjtest: vgalizer pid $VPID (parent $VJPID)"

# --- Background RSS sampler --------------------------------------------------
(
    while kill -0 "$VPID" 2>/dev/null; do
        if [ -r "/proc/$VPID/status" ]; then
            ts=$(date +%H:%M:%S)
            rss=$(awk '/^VmRSS:/{print $2}' "/proc/$VPID/status" 2>/dev/null)
            threads=$(awk '/^Threads:/{print $2}' "/proc/$VPID/status" 2>/dev/null)
            printf "%s rss_kb=%s threads=%s\n" "$ts" "${rss:-?}" "${threads:-?}"
        fi
        sleep "$RSS_INTERVAL"
    done
) >"$LOGDIR/rss.log" &
MPID=$!

# --- Summary printer (runs on any exit path, including ^C) ------------------
summarize() {
    kill "$MPID" 2>/dev/null
    # Let any last RSS write flush.
    sleep 0.2

    {
        echo "=== vjtest summary ($MODE, requested ${DURATION}s) ==="
        echo "logs:    $LOGDIR"
        echo "started: $(head -1 "$LOGDIR/rss.log" 2>/dev/null | awk '{print $1}')"
        echo "ended:   $(tail -1 "$LOGDIR/rss.log" 2>/dev/null | awk '{print $1}')"
        echo "exit:    $EXIT_CODE"
        echo
        echo "--- RSS (kB) ---"
        awk -F'rss_kb=' '
            NF==2 {
                split($2, p, " "); v=p[1]+0;
                if (NR==1 || min==0 || v<min) min=v;
                if (v>max) max=v;
                last=v;
            }
            END { printf "  min=%d  max=%d  last=%d  delta=%d\n",
                         min, max, last, max-min }
        ' "$LOGDIR/rss.log"
        echo
        # vgalizer uses env_logger which writes to stderr, so perf / beat /
        # scene / reload lines all land in stderr.log, not stdout.log. Grep
        # there. Also: `grep -c` exits 1 when it finds 0 matches, which
        # combined with `|| echo 0` used to print "0\n0" — use awk for the
        # counters so we always get a single clean integer.
        LOG="$LOGDIR/stderr.log"
        count() { awk -v pat="$1" '$0 ~ pat {n++} END{print n+0}' "$LOG" 2>/dev/null; }

        echo "--- perf: lines (last 10) ---"
        grep "perf:" "$LOG" 2>/dev/null | tail -10
        echo "  (total perf lines: $(count 'perf:'))"
        echo
        echo "--- beat lock events ---"
        grep "beat:" "$LOG" 2>/dev/null | tail -20
        echo "  (locks: $(count 'beat: locked'), drops: $(count 'beat: lock dropped'))"
        echo
        echo "--- config reloads ---"
        grep -E "reload:|render targets:" "$LOG" 2>/dev/null | tail -10
        echo
        echo "--- scene switches (count) ---"
        echo "  $(count 'Scene:')"
        echo
        echo "--- warnings / errors / panics ---"
        grep -iE "warn|error|panic|fatal" "$LOGDIR/stderr.log" "$LOGDIR/stdout.log" \
            2>/dev/null | head -30
        echo
    } | tee "$LOGDIR/summary.txt"
}

# Ensure summary runs whether we ^C, SIGTERM, or the sleep finishes.
EXIT_CODE=0
trap 'EXIT_CODE=130; summarize; exit $EXIT_CODE' INT TERM

# --- Wait for the requested duration, then shut vgalizer down cleanly --------
sleep "$DURATION" &
SLEEP_PID=$!
wait "$SLEEP_PID" 2>/dev/null
# If vgalizer died during the wait, pick that up.
if ! kill -0 "$VPID" 2>/dev/null; then
    EXIT_CODE=$(wait "$VJPID" 2>/dev/null; echo $?)
    summarize
    exit "$EXIT_CODE"
fi

# Clean shutdown: TERM first, then KILL after 3s grace.
kill -TERM "$VJPID" 2>/dev/null
for _ in 1 2 3; do
    kill -0 "$VJPID" 2>/dev/null || break
    sleep 1
done
kill -KILL "$VJPID" 2>/dev/null

wait "$VJPID" 2>/dev/null
EXIT_CODE=$?
summarize
exit "$EXIT_CODE"
