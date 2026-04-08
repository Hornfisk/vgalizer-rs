# Changelog

## 2026-04-08 — T1–T6b debug + feature batch (from lazy-roaming-sky plan)

Seven commits landed in one session covering two user-reported bugs
(choppy effects post-reboot, E/G overlay input lag), three issues found
during investigation (beat detector never locks, SIGTERM panic, HUD
stats feature request), and one hardware feature (HDA jack hot-swap).
Full root-cause analysis and per-effect 600 s headless baselines live
in `~/lazy-roaming-sky.md` on the dev box.

### Added
- **HUD system stats** (T5). New `src/system_stats.rs` module spawns a
  background `vgalizer-sysstats` thread that polls `/proc/stat`,
  `/proc/self/status`, `/proc/meminfo`, `/sys/class/thermal/thermal_zone{0,2}/temp`,
  and `/sys/class/drm/card0/gt_{cur,max}_freq_mhz` at 1 Hz. Results
  publish through atomics to the render loop, so HUD read is lock-free.
  HUD grows a middle line: `FPS xx.x  CPU xx%  T xx°/xx°  RAM xx/xxxx MB  GPU xxxx/xxxx MHz`.
  FPS is fed from the existing rolling perf window in `src/app/frame.rs`.
  All sysfs paths verified on pingo's ThinkPad E480 (UHD 620, Debian 13);
  thermal_zone0 is acpitz, thermal_zone2 is x86_pkg_temp (best iGPU die
  proxy), thermal_zone1 is pch_skylake and intentionally ignored.
- **HDA jack auto-swap** (T6b). New `src/audio/jack_detect.rs` uses the
  `evdev` crate (new dep, version 0.12) to watch the `HDA Intel PCH Mic`
  input device for `SW_MICROPHONE_INSERT` events. On plug/unplug the
  audio capture stream is torn down and restarted so the pipewire
  auto-pick chain re-routes to whatever source is now active. Dedicated
  watcher thread blocks on `fetch_events` — zero CPU between plug
  events. New config field `audio_auto_swap: bool` (default `true`).
  Setting `audio_device` explicitly disables the watcher so user
  overrides are always respected. Gracefully no-ops on non-HDA hardware.
  No root or udev rules needed — the Linux `input` group grants read on
  `/dev/input/event*`.
- **Effect pipeline prewarm at startup** (T2). `EffectRegistry::prewarm`
  issues a throwaway draw for every pipeline into a real color target,
  submits, and calls `device.poll(Wait)` so Mesa iris can't defer the
  shader compile until first in-scene draw. Logs
  `prewarm: compiled N effect pipelines in X.X ms`. Kills the
  first-visit hitch every time the scene manager rotates to a new
  effect.
- **Beat tracker debug instrumentation** (T6a). `beat.rs` now logs a
  `beat-dbg:` line at `debug` level once every 60 update calls (~1 Hz):
  rolling flux peak, current `interval`, computed BPM, lock state,
  `consec_in_window`, `recent_intervals` length, mean, and stddev, plus
  the missed-beat count. No constants tuned — strictly measurement, so
  tuning the `LOCK_STDDEV_MAX` threshold after reading a known-BPM run
  is a follow-up decision. Gated on `RUST_LOG=vgalizer=debug`.
- **E/G overlay input-latency instrumentation** (T4). Every E
  (param editor) or G (global settings) action tags an `Instant` in
  `AppState.pending_overlay_input`; the next rendered frame logs
  `overlay-input-latency: {E|G} x.xx ms`. Lets pingo measure the
  key-press → paint interval directly instead of guessing.

### Changed
- **Overlay text caching** (T4). HUD, params, effects-menu, and
  global-settings overlays now compare each newly-built text string
  against a cached `last_text` and skip `buffer.set_text` +
  `shape_until_scroll` when identical. Root cause of the E/G input lag
  was `frame.rs` running full glyphon reshapes every frame on unchanged
  text (~60 reshapes/sec per open overlay). VjeOverlay (V unified
  overlay) is explicitly out of scope for this fix and intentionally
  untouched. See T4 notes in `~/lazy-roaming-sky.md`.
- **`voronoi_pulse.wgsl`** `NUM_POINTS` 42 → 20 and
  **`vector_terrain.wgsl`** `ROWS` 36 → 22 (T2c). Both shaders were
  ALU-bound on UHD 620 at 720p (p50 = 37.75 ms and 24.98–37.41 ms
  respectively in the 600 s baseline), not fill-rate or pipeline
  cold-compile as initially suspected. Conservative cuts that preserve
  visual identity; further reductions are the next step if 1080p is
  still hot.

### Fixed
- **SIGTERM panic in the winit event loop runner** (T1). `event_loop.run_app(...)`
  returns `Err` on SIGTERM instead of `Ok(())`, so the previous
  `.expect()` was crashing the process with a stack trace on any
  graceful shutdown. Replaced with an `if let Err` logger so `kill -TERM`
  and `^C` exit cleanly. Affects `src/app/mod.rs::run`.

### Dependencies
- Added `evdev = "0.12"` for HDA jack watcher (T6b).

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
