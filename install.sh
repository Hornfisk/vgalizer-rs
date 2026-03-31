#!/usr/bin/env bash
# One-liner installer for vgalizer.
# Usage: curl -sSf https://raw.githubusercontent.com/Hornfisk/vgalizer-rs/master/install.sh | sh

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

# ---- Build & install ----
echo "Building vgalizer (this takes a minute on first run)..."
cargo install --git https://github.com/Hornfisk/vgalizer-rs vgalizer

# ---- Add shell aliases (idempotent) ----
add_aliases() {
    local rc="$1"
    if [[ -f "$rc" ]] && grep -q 'vgalizer-rs aliases' "$rc" 2>/dev/null; then
        return  # already present
    fi
    if [[ -f "$rc" ]]; then
        cat >> "$rc" <<'EOF'

# >>> vgalizer-rs aliases >>>
alias vgr='vgalizer'
alias vgrname='vgalizer --name'
alias vgrw='vgalizer --windowed'
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
echo "  vgalizer --list-audio     → see available audio devices"
