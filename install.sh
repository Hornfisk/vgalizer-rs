#!/usr/bin/env bash
# One-liner install / update for vgalizer + vje.
#
#   curl -sSfL https://raw.githubusercontent.com/Hornfisk/vgalizer-rs/master/install.sh | bash
#
# Idempotent: first run installs, subsequent runs update to latest master.
# Safe to re-run — rust is only installed if missing, system deps are
# best-effort, shell aliases are guarded by a marker comment.

set -e

# ---- Install Rust if missing ----
if ! command -v cargo &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    # shellcheck source=/dev/null
    . "$HOME/.cargo/env"
fi

# ---- Install system audio deps (best-effort, skip on failure) ----
if command -v apt-get &>/dev/null; then
    sudo apt-get install -y --no-install-recommends libasound2-dev libvulkan1 2>/dev/null || true
elif command -v pacman &>/dev/null; then
    sudo pacman -Sy --noconfirm alsa-lib vulkan-icd-loader 2>/dev/null || true
elif command -v dnf &>/dev/null; then
    sudo dnf install -y alsa-lib-devel vulkan-loader 2>/dev/null || true
fi

# ---- Build & install (or update) ----
# --force makes this idempotent: first run installs, later runs update from
# remote master if anything changed. --bins installs every binary in the
# crate, so both `vgalizer` and `vje` end up in ~/.cargo/bin.
echo "Building vgalizer + vje (this takes a minute on first run)..."
cargo install --git https://github.com/Hornfisk/vgalizer-rs vgalizer --bins --force

# ---- Add shell aliases (idempotent) ----
add_aliases() {
    local rc="$1"
    if [[ -f "$rc" ]] && grep -q 'vgalizer-rs aliases' "$rc" 2>/dev/null; then
        return  # already present
    fi
    if [[ -f "$rc" ]]; then
        cat >> "$rc" <<'EOF'

# >>> vgalizer-rs aliases >>>
vgr()  { vgalizer ${1:+--name "$1"} "${@:2}"; }
vgrw() { vgalizer --windowed ${1:+--name "$1"} "${@:2}"; }
vje()  { command vje "$@"; }   # standalone TUI param editor
# <<< vgalizer-rs aliases <<<
EOF
        echo "Added aliases to $rc"
    fi
}

add_aliases "$HOME/.bashrc"
add_aliases "$HOME/.zshrc"

echo ""
echo "Done!  Reload your shell:  source ~/.zshrc  (or ~/.bashrc)"
echo ""
echo "  vgr                       → fullscreen with name from config"
echo "  vgrname \"YOUR DJ NAME\"    → fullscreen with custom name"
echo "  vgrw                      → windowed (for testing)"
echo "  vje                       → live TUI param editor (runs alongside vgalizer)"
echo "  vgalizer --list-audio     → see available audio devices"
