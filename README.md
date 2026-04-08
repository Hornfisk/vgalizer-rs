# vgalizer

Lightweight GPU-accelerated audio-reactive DJ visualizer, written in Rust.

Designed for live use on a projector or screen behind the DJ booth — minimal dependencies, fast cold start, audio-reactive without setup, hot-reloadable config, standalone TUI param editor for live tweaking.

## Install / update (one-liner)

```bash
curl -sSfL https://raw.githubusercontent.com/Hornfisk/vgalizer-rs/master/install.sh | bash
```

Idempotent — first run installs Rust (if missing), audio system deps, both binaries (`vgalizer` + `vje`), and shell aliases. Re-running updates everything from latest master.

After install, reload your shell (`source ~/.zshrc` or `source ~/.bashrc`) and:

- `vgr` — run fullscreen with name from config
- `vgrw` — run windowed (for testing)
- `vje` — open the live TUI param editor (run alongside `vgr`)

## Features

- **25 GPU effects** across three sets — v1 classics (hyperspace, kaleido, ring tunnel, warp grid, morph geo, spectrum bars/orbit/terrain/wave), v2 techno/op-art (line_moire, mandelbrot_zoom, wire_tunnel, voronoi_pulse), and v3 vector/scope/cymatics (vector_terrain, laser_burst, scope_xy, wave_dunes, radial_eq, harmonograph, tv_acid, kaleido_warp, isoline_field, moebius_grid, cymatics, vector_rabbit).
- **Post-process chain** — trail, glitch, mirror (H/V/quad/kaleido pool), global rotation + vibration, scanlines, strobe, VGA chromatic-aberration / sync / noise.
- **Audio reactive** — multi-backend capture (cpal ALSA, parec for PulseAudio monitor sources, pw-cat for native PipeWire) with auto-selection. RMS level + 32-band FFT + beat tracker expose `level / pulse / bpm / bands[i]` to every shader.
- **Live DJ name overlay** — auto-fits to screen width, chromatic aberration, beat-synced pulse + jitter. Editable live (press `T`).
- **vje — standalone TUI param editor** — separate binary (`~/.cargo/bin/vje`). Browse all 26 effects and their params, nudge values with `←→` (Shift = ×10), reset to defaults, toggle effects on/off, edit globals (scene duration, beat sensitivity, strobe mode, fx speed, mirror pool…). Commits land in `~/.config/vgalizer/config.json` atomically; the running visualizer picks them up via `notify` in ~100ms — no restart.
- **Hot-reload config with layered merge** — seed `config.json` ships the baseline shape, user state lives in `~/.config/vgalizer/config.json`. Both files are merged on startup AND on every external edit, so `vje`, in-app overlays, or manual edits all feed the same live-reload path.
- **In-app menus** — press `E` for per-effect params, `G` for global knobs (trail, VGA, mirror, glitch, rotation, vibration), `M` to disable effects you don't want in rotation, `A` for the audio device picker.
- **Scene manager** — auto-cycles effects + palette + mirror mode every N seconds; randomizes per-effect parameters on switch; honors the `disabled_effects` deny list.
- **Low overhead** — wgpu 24, single fullscreen triangle per effect, runs at 60 FPS on integrated graphics.

## Manual build (without the one-liner)

```bash
git clone https://github.com/Hornfisk/vgalizer-rs
cd vgalizer-rs
cargo install --path . --bins --force
```

### CLI flags

```
vgalizer [--windowed] [--name "DJ NAME"] [-c config.json] [-d "audio device"] [-r 1920x1080]
vgalizer --list-audio    # enumerate input devices
```

## Keybindings (live, in the visualizer)

| Key | Action |
|---|---|
| `SPACE` | Next effect |
| `1`–`9` | Jump to effect N |
| `+` / `-` / `↑` / `↓` | Beat sensitivity up / down |
| `P` | Cycle post mirror mode |
| `A` | Toggle audio device picker |
| `E` | Per-effect params menu |
| `G` | Global knobs menu (trail, VGA, mirror, rotation, vibration, glitch) |
| `M` | Effects enable/disable menu |
| `T` | Edit DJ name live (Enter saves, Esc cancels) |
| `H` | Toggle HUD |
| `F` / `W` | Fullscreen / windowed |
| `Q` / `Esc` | Quit |

## Keybindings (vje — the TUI editor)

Run `vje` in any terminal while `vgalizer` is running. Two processes, one config file — edits apply live.

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Switch Effects ↔ Globals tab |
| `↑` `↓` / `j` `k` | Move cursor |
| `Enter` (on effect) | Open param editor for that effect |
| `←` `→` / `h` `l` | Nudge value by `step` (hold `Shift` for ×10) |
| `Enter` (on param/global) | Commit all pending edits to disk |
| `R` | Reset hovered value to default |
| `X` | Toggle effect disable/enable |
| `Esc` (in param editor) | Back to effect list |
| `Esc` (elsewhere) | Revert all uncommitted edits |
| `q` / `Ctrl+C` | Quit (warns if dirty) |
| `?` | Help overlay |

## Config

`config.json` in the working directory = seed baseline (repo layer).
`~/.config/vgalizer/config.json` = user state, atomically overlaid at startup and on every external edit.

```json
{
  "dj_name": "DJ NAME",
  "fullscreen": true,
  "audio_device": null,
  "scene_duration": 30.0,
  "beat_sensitivity": 1.4,
  "strobe_mode": "beat",
  "fx_speed_mult": 1.0,
  "mirror_pool": ["none","none","mirror_h","mirror_v","mirror_quad","kaleido"],
  "disabled_effects": null,
  "fx_params": { "kaleido": { "spokes": 0.75, "rings": 1.0 } }
}
```

- `disabled_effects: null` rotates all 26 effects. Set to a list (deny list) to skip specific ones — new effects added in code updates stay enabled by default.
- `fx_params.<effect>.<param>` overrides the default for that single param. Normally managed by `vje` or the `E` menu, but the file is human-readable if you want to hand-edit.
- The watcher watches the **parent directory** of the XDG file (not the file itself) so atomic renames from `vje`, vim, or any other editor survive the reload.

## Architecture

```
src/
  lib.rs                 pub mod re-exports for the vje bin
  main.rs                vgalizer bin entry
  bin/vje/               vje bin — standalone ratatui TUI param editor
    main.rs              crossterm event loop
    state.rs             AppState, dirty tracking, GLOBAL_ROWS
    edit.rs              nudge / reset / commit (wraps write_xdg_fields)
    ui.rs                draw: effects list, param table, globals, help
  app.rs                 winit event loop, per-frame pipeline, watcher reload block
  gpu/                   wgpu context, pipelines, GlobalUniforms/EffectUniforms/PostUniforms
  effects/               EffectRegistry + SceneManager + ParamDef schema
  postprocess/           Trail, glitch, mirror, rotation, scanlines, strobe, VGA
  audio/                 cpal + parec + pw-cat capture, FFT analysis, BeatTracker
  text/                  Glyphon-backed NameOverlay + TextInputOverlay
  overlay.rs             HUD
  input.rs               keyboard action mapping, picker-mode routing
  audio_picker.rs        A-menu picker (ALSA + PA + PW sources)
  effects_menu.rs        M-menu effect toggle list
  global_settings.rs     G-menu global knobs
  config/
    schema.rs            Config struct (serde)
    mod.rs               load_merged() + atomic writers (write_xdg_fields, ...)
    watcher.rs           parent-dir inotify watcher, debounced reload
shaders/
  globals.wgsl           Shared uniforms + helpers, prepended to every effect
  fullscreen.wgsl        Vertex shader (single fullscreen triangle)
  effects/*.wgsl         One per effect (26 files)
  post/*.wgsl            Post chain
previews/                Standalone WebGL2 HTML previews of each effect
```

### Adding a new effect

1. Create `shaders/effects/my_effect.wgsl` with a `@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32>` entry point.
2. Globals are auto-prepended — use `globals.time`, `globals.level`, `globals.pulse`, `band(i)`, `param(i)`, `smooth_pulse()`.
3. Register the name in `src/effects/mod.rs` — add to `EFFECT_NAMES` and `effect_source()`.
4. (Optional) Add a `ParamDef` entry in `src/effects/params.rs` so the knobs appear automatically in `vje` and the in-app `E` menu.
5. Rebuild — the scene manager picks it up automatically.

## Dependencies

GPU: wgpu 24 · winit 0.30
Audio: cpal 0.15 (+ parec / pw-cat subprocess fallback)
FFT: rustfft 6
Text: glyphon 0.8
TUI (vje): ratatui 0.29 · crossterm 0.28

## Soak testing

`scripts/vjtest.sh` is a bounded, monitored soak-run harness. Three modes:

- `vjtest.sh <secs> headless` — `WLR_BACKENDS=headless cage -- vgalizer`. SSH-safe, no physical display. Exercises the full render + post + blit pipeline with no scanout. Good for catching leaks, panics, and FFT/beat detector regressions before a gig.
- `vjtest.sh <secs> windowed` — native `vgalizer --windowed 1280x720`. Requires an existing Wayland/X11 session.
- `vjtest.sh <secs> cage` — real cage kiosk on the physical tty. Production path.

Captures stdout/stderr, periodic RSS samples, and on exit prints a summary with peak/delta RSS, fps percentiles from the `perf:` log lines, beat-lock counts, scene-switch count, and any warnings/errors. `^C` is safe — the trap runs the summarizer on any exit path. Suggested shell aliases:

```bash
alias vjtestshort='~/repos/vgalizer-rs/scripts/vjtest.sh 60 headless'
alias vjtestfull='~/repos/vgalizer-rs/scripts/vjtest.sh 2700 headless'
alias vjtestcage='~/repos/vgalizer-rs/scripts/vjtest.sh 2700 cage'
```

## Running on a dedicated kiosk box (ravebox / pingo)

See [`ravebox-runbook.md`](ravebox-runbook.md) for the full build of a sealed-purpose ThinkPad E480 running Debian 13 + PipeWire + cage + vgalizer as the sole workload. Covers distro choice rationale, PipeWire lockdown config, audio line-in → mic fallback, reliability locks for live use, boot flow, and the dual-display mirror workaround (stock cage 0.2.0 has no mirror mode — a 1-line patch to `output.c`'s `output_layout_add_auto` pins every output to `(0,0)` which gives true mirroring via wlroots' scene graph).

## License

MIT
