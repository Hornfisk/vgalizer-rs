# ravebox / pingo — project CLAUDE.md

You are running on **pingo** (ThinkPad E480, Debian 13 Trixie), the live visualizer box.
This is NOT the Arch desktop. Do not assume memory-search, the Obsidian vault, pacman,
freshclam, hyprctl, or any Arch-specific tooling exists here. Package manager is `apt`.

The ravebox project memory at `~/.claude/projects/-home-natalia-repos-vgalizer-rs/memory/`
is the canonical state. Read `rave-box-pingo.md` for the locked decisions, completed
phases, and the current status. Read `./ravebox-runbook.md` in this dir for the long-form
Obsidian-style runbook with the full history of the box build.

## Current state (2026-04-08)

- Debian 13 + PipeWire + cage + vgalizer pipeline is fully working on the physical tty.
- Test ladder (all green): `vjtestshort` (1 min headless) → `vjtestfull` (45 min
  headless) → `vjtestcage` (cage on tty with real DRM outputs).
- **Dual-display mirror is live.** cage 0.2.0 has no native mirror mode, so the
  Debian `cage 0.2.0-2` source was patched locally in `output.c` to pin every
  output to `(0,0)` via the existing `output_layout_add(output, 0, 0)` helper.
  The patched binary is installed at `/usr/bin/cage` and held with
  `apt-mark hold cage`. Rebuild path and full context in
  `~/.claude/projects/-home-natalia-repos-vgalizer-rs/memory/rave-box-pingo.md`.
- Next: Phase 7 autologin on tty1 → cage → vgalizer as a systemd unit so the box
  boots straight into the visualizer with no interactive login.

## Hard rules on pingo

- Editor: **vim only**.
- Prefer single-line commands (no `\` continuations) — paste reliability over SSH
  matters here.
- Before recommending installs, run `dpkg -l <pkg>` to check if it's already there.
- When in doubt about hardware state, prefer `loginctl`, `journalctl -b`, `dmesg`,
  `wpctl status`, `pw-cli info all` over guessing.
- Invoke cage as `cage -- bash -c "cd <dir> && exec <binary>"` — the `bash -c` wrapper
  side-steps an old `cage.c:138` execvp-failure mode and keeps the working directory
  right. `vj` and `vjtest.sh` already do this; don't shortcut it to
  `cage -- <binary>`.
- The cage binary on this box is a **locally patched 0.2.0-2**. Do not
  `apt install --reinstall cage` or remove the hold without reapplying the
  mirror patch afterwards — the stock Debian binary will cascade eDP-1 and
  HDMI-A-1 side-by-side instead of mirroring them.
