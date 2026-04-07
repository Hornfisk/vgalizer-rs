# Changelog

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
