# Changelog

## 2026-04-08 — ravebox dual-display mirror + soak test tooling

### Added
- **`scripts/vjtest.sh`** — bounded, monitored soak-run harness for vgalizer.
  Three modes: `headless` (SSH-safe via `WLR_BACKENDS=headless cage -- vgalizer`,
  exercises the full render + post + blit pipeline with no physical display),
  `windowed` (native `vgalizer --windowed` inside an existing session), and
  `cage` (real cage kiosk on the physical tty, the production path).
  Captures stdout/stderr, periodic RSS samples, and on exit prints a summary
  with peak RSS, fps from the `perf:` log lines, beat-lock counts,
  scene-switch count, and any warnings/errors. `^C` safe — the trap runs
  the summarizer on any exit path. Shell aliases: `vjtestshort` (1 min
  headless), `vjtestfull` (45 min headless), `vjtestcage` (45 min cage tty).
- **`CLAUDE.md`** — project-level Claude instructions documenting the pingo
  stack (Debian 13, cage, PipeWire), the cage invocation pattern
  (`cage -- bash -c "cd <dir> && exec <bin>"`), and hard rules for working
  on the live box (vim only, single-line SSH pastes, no Arch assumptions).
- **`ravebox-runbook.md`** — the long-form operational runbook for pingo
  (the ThinkPad E480 live visualizer box): install history, PipeWire
  lockdown config, hardware notes, boot flow, reliability locks, and
  phase checklist. Lives in-repo so it travels with the code.

### Fixed
- **`scripts/vjtest.sh` summary now reads from stderr.** vgalizer's
  `env_logger` writes to stderr by default, so the `perf:`, `beat:`,
  `Scene:`, and `reload:` greps the summarizer was running against
  `stdout.log` always came back empty even on healthy runs. Moved the
  greps to `stderr.log` and replaced `grep -c … || echo 0` (which
  printed `0\n0` when grep matched zero lines) with an `awk` counter
  that always returns a clean integer.

### Ravebox (local to pingo, not in this repo)
- **cage 0.2.0-2 patched for dual-display mirror.** Stock cage on Debian 13
  has no mirror mode — `-m extend` cascades outputs side-by-side, `-m last`
  uses only the last connector. On the E480 with eDP-1 (1366×768 laptop
  panel) + HDMI-A-1 (1366×768 to 720p projector) both connected, extend
  mode splits the vgalizer view across the two panels. Fix: patched the
  Debian source package's `output.c` — replaced the body of
  `output_layout_add_auto()` to pin every output to `(0,0)` via the
  existing `wlr_output_layout_add(..., 0, 0)` helper, so wlroots' scene
  graph renders the same scene into both scene_outputs. Installed at
  `/usr/bin/cage`, held with `apt-mark hold cage`. Rebuild workspace
  and `install.py` one-shot installer live at
  `/home/natalia/build/cage-mirror/` on pingo.

## 2026-04-07

### Added
- **`vje` — standalone TUI param editor** (new binary). ratatui + crossterm. Effects browser + per-effect param editor (`←→` nudge, `Shift`=×10, `R`=reset, `X`=disable) + globals tab (`scene_duration`, `beat_sensitivity`, `strobe_mode`, `fx_speed_mult`, `vga_sync`, `mirror_count`, `mirror_spread`, `dj_name` text input) + help overlay. Commits edits atomically to `~/.config/vgalizer/config.json` via `write_xdg_fields`; the running visualizer picks them up through the existing `notify` watcher in ~100ms.
- **`src/lib.rs`** re-exports every top-level module so the `vje` binary can import as `use vgalizer::{config, effects, audio, ...}`.
- **`[[bin]]` entries** for both `vgalizer` and `vje` in `Cargo.toml`.
- **One-liner install/update** via `install.sh` now supports updating an existing install (`cargo install ... --force`) and builds both binaries (`--bins`). Re-running `curl -sSfL .../install.sh | bash` installs on a fresh box or updates an existing one; shell aliases are guarded by a marker comment so they're added once.
- **`vje()` shell function** added alongside `vgr`/`vgrw` by `install.sh`.
- **README vje section** — install one-liner at the top, full vje keymap table, updated architecture tree, effect count corrected to 26, `disabled_effects` replaces the stale `enabled_effects` reference.

### Changed
- `Cargo.toml` — added `ratatui 0.29` and `crossterm 0.28` dependencies.
- `install.sh` — `cargo install` now uses `--bins --force` for update semantics; aliases block now includes `vje()`; intro comment rewritten to document the update use case.

## 2026-04-07 — Live-reload all config fields from XDG edits (commit 783e6a0)

### Added
- **`load_merged(path)`** in `src/config/mod.rs` — single code path for seed + XDG merge, called both at startup AND by the watcher, so external edits pick up identically.
- **Parent-directory inotify watch** in `src/config/watcher.rs` — the watcher now watches the parent directory of the XDG config file, not the file itself, so atomic rename-over saves from vim / `vje` / any editor survive the reload. Events are filtered by filename and debounced 100ms.
- **Reload wiring in `app.rs`** — `scene_duration`, `mirror_pool`, and `disabled_effects` now re-apply on live reload (previously they were only read at scene construction).
- **`SceneManager::set_mirror_pool`** — needed for the live reload path.

### Removed
- 9 dead schema fields with no `src/` references: `target_fps`, `name_font_size`, `global_vib_division`, `kaleido_post_alpha`, `spectrum_n_bands`, `spectrum_height`, `spectrum_glow`, `spectrum_color`, `spectrum_anchor`.
- The dead `_font_size_frac` param from `NameOverlay::new` — font size has always been auto-fit from screen dimensions; that param was never read.

### Verified
End-to-end headless test: vgalizer running in Hyprland `special:hidden` workspace, atomic-rename config edits via Python → 4 edits → 4 reloads → 3 setter log lines + 1 silent state-swap. All four live-reload paths confirmed working.

## Earlier

### Added
- **5 new GPU effects** for techno/op-art sets:
  - `line_moire` — three interfering line fields with lens warp
  - `mandelbrot_zoom` — continuous infinite zoom into Seahorse Valley
  - `strange_attractor` — fragment-shader Clifford attractor with continuous parameter drift
  - `wire_tunnel` — wormhole flythrough with longitudinal neon stripes
  - `voronoi_pulse` — 42 seeds orbiting on incommensurate Lissajous paths
- **Live DJ name editing** — press `T` to open a centered text input overlay; Enter saves to config, Esc cancels, Backspace erases.
- **DJ name auto-fit** — rasterizes once at 256px and scales per-frame to fill screen width with a small side margin; no more fixed font-size fraction.
- **HTML effect previews** under `previews/` — standalone WebGL2 previews of each effect with simulated 120 BPM audio.
- **README.md** — features, quick start, keybindings, config, architecture overview.

### Changed
- HUD shortcut line now shows `T name`.
- HUD level bar uses ASCII `#`/`-` instead of Unicode block chars (the bundled font has no block glyphs).
- `write_audio_device` / `write_dj_name` now share a generic `write_string_field_to_path` helper.

### Fixed
- ALSA library chatter (`dmix`, `dsnoop`, `oss` plugin warnings) no longer spams stderr on startup — cpal device probing and stream construction run under a temporary stderr→`/dev/null` redirect.
- `T` key now routes raw keystrokes to the text editor instead of triggering effect shortcuts while the input is open.

### Dependencies
- Added `libc = "0.2"` for the stderr-silencing FD dance.
