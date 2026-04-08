#!/usr/bin/env bash
# tune-sync — move tuning fields between the repo seed (config.json) and the
# user's XDG state (~/.config/vgalizer/config.json) without dragging along
# machine-specific fields like audio_device, fullscreen, spectrum_*.
#
# Usage:
#   tune-sync push          # XDG → repo seed (run before git commit)
#   tune-sync pull           # repo seed → XDG (run after git pull to activate)
#   tune-sync diff           # show what 'push' *would* change, don't touch anything
#
# Fields synced (allowlist, everything else is ignored in both directions):
#   disabled_effects, fx_params, beat_sensitivity, scene_duration,
#   mirror_pool, dj_name
#
# Live-reload safe: writes via atomic rename, picked up by the config watcher
# without restarting vgalizer.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SEED="$REPO_ROOT/config.json"
XDG="${XDG_CONFIG_HOME:-$HOME/.config}/vgalizer/config.json"

ALLOWLIST='{disabled_effects, fx_params, beat_sensitivity, scene_duration, mirror_pool, dj_name} | with_entries(select(.value != null))'

die() { echo "tune-sync: $*" >&2; exit 1; }

command -v jq >/dev/null || die "jq not installed"
[[ -f "$SEED" ]] || die "seed not found: $SEED"

merge() {
    # merge <base> <overlay-with-allowlist> → stdout
    # $1 = base JSON (target shape), $2 = overlay JSON (source of tuning)
    jq -s ".[0] * (.[1] | $ALLOWLIST)" "$1" "$2"
}

case "${1:-}" in
    push)
        [[ -f "$XDG" ]] || die "XDG not found: $XDG (nothing to push from)"
        tmp="$(mktemp "${SEED}.XXXXXX")"
        merge "$SEED" "$XDG" > "$tmp"
        mv "$tmp" "$SEED"
        echo "tune-sync: XDG → seed ($SEED)"
        if command -v git >/dev/null && git -C "$REPO_ROOT" rev-parse &>/dev/null; then
            echo "---"
            git -C "$REPO_ROOT" --no-pager diff -- "$(basename "$SEED")" | head -100
            echo "---"
            echo "Review, then: git -C $REPO_ROOT commit -am '…' && git push"
        fi
        ;;
    pull)
        [[ -f "$XDG" ]] || { mkdir -p "$(dirname "$XDG")"; cp "$SEED" "$XDG"; echo "tune-sync: XDG created from seed"; exit 0; }
        tmp="$(mktemp "${XDG}.XXXXXX")"
        merge "$XDG" "$SEED" > "$tmp"
        mv "$tmp" "$XDG"
        echo "tune-sync: seed → XDG ($XDG) — live reload should pick it up"
        ;;
    diff)
        [[ -f "$XDG" ]] || die "XDG not found: $XDG"
        tmp="$(mktemp)"
        merge "$SEED" "$XDG" > "$tmp"
        diff -u "$SEED" "$tmp" || true
        rm -f "$tmp"
        ;;
    *)
        cat >&2 <<EOF
Usage: $(basename "$0") {push|pull|diff}

  push   XDG (~/.config/vgalizer/config.json) → repo seed (config.json)
         Run this before committing tuning changes.
  pull   repo seed → XDG
         Run this after 'git pull' to activate new tuning on this machine.
  diff   show what 'push' would change, without touching any file.
EOF
        exit 2
        ;;
esac
