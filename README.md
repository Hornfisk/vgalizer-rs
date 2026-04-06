# vgalizer

Lightweight GPU-accelerated audio-reactive DJ visualizer, written in Rust.

Designed for live use on a projector or screen behind the DJ booth — minimal dependencies, fast cold start, audio-reactive without setup, hot-reloadable config.

## Features

- **14 GPU effects** — 9 original (hyperspace, kaleido, ring tunnel, warp grid, morph geo, spectrum bars/orbit/terrain/wave) + 5 new techno/op-art effects (line_moire, mandelbrot_zoom, strange_attractor, wire_tunnel wormhole, voronoi_pulse).
- **Post-process chain** — trail, glitch, mirror, rotation, scanlines, strobe, VGA chromatic-aberration.
- **Audio reactive** — cpal capture → FFT band analysis → beat tracking (level / pulse / bpm / 32 bands exposed to every shader).
- **Live DJ name overlay** — auto-fits to screen width, chromatic aberration, beat-synced pulse + jitter. Editable live (press `T`).
- **Hot-reload config** — edit `config.json`, changes apply instantly.
- **Scene manager** — auto-cycles effects + palette + mirror mode every N seconds; randomizes per-effect parameters on switch.
- **Audio device picker** — overlay UI, writes choice back to config.
- **Low overhead** — wgpu 24, single fullscreen triangle per effect, runs at 60 FPS on integrated graphics.

## Quick start

```bash
cargo build --release
./target/release/vgalizer
```

First run will pick the system default audio input. Press `A` to open the device picker if you want something else.

### CLI flags

```
vgalizer [--windowed] [--name "DJ NAME"] [-c config.json] [-d "audio device"] [-r 1920x1080]
```

## Keybindings (live)

| Key | Action |
|---|---|
| `SPACE` | Next effect |
| `1`–`9` | Jump to effect N |
| `+` / `-` / `↑` / `↓` | Beat sensitivity up / down |
| `P` | Cycle post mirror mode |
| `A` | Toggle audio device picker |
| `T` | **Edit DJ name live** (Enter saves, Esc cancels, Backspace erases) |
| `H` | Toggle HUD |
| `F` / `W` | Fullscreen / windowed |
| `Q` / `Esc` | Quit |

## Config

`config.json` in the working directory, or `~/.config/vgalizer/config.json`:

```json
{
  "dj_name": "DJ NAME",
  "fullscreen": true,
  "target_fps": 60,
  "audio_device": null,
  "scene_duration": 30.0,
  "beat_sensitivity": 1.4,
  "enabled_effects": null
}
```

- `enabled_effects: null` rotates all 14 effects. To restrict, pass a list, e.g.:
  ```json
  "enabled_effects": ["line_moire", "wire_tunnel", "mandelbrot_zoom", "voronoi_pulse", "warp_grid"]
  ```
- Editing `dj_name` in the file while running updates the name live (or just press `T`).

## Architecture

```
src/
  app.rs               Winit event loop, per-frame pipeline
  gpu/                 wgpu context, pipelines, GlobalUniforms/EffectUniforms/PostUniforms
  effects/             EffectRegistry + SceneManager (auto-cycles)
  postprocess/         Trail, glitch, mirror, rotation, scanlines, strobe, VGA chain
  audio/               cpal capture, FFT analysis, BeatTracker
  text/                Glyphon-backed NameOverlay + TextInputOverlay
  overlay.rs           HUD (F1)
  config/              JSON config + hot-reload (notify)
shaders/
  globals.wgsl         Shared uniforms + helpers, prepended to every effect
  fullscreen.wgsl      Vertex shader (single fullscreen triangle)
  effects/*.wgsl       One per effect
  post/*.wgsl          Post chain
previews/              Standalone WebGL2 HTML previews of each effect (audition in browser)
```

### Adding a new effect

1. Create `shaders/effects/my_effect.wgsl` with a `@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32>` entry point.
2. Globals are auto-prepended — use `globals.time`, `globals.level`, `globals.pulse`, `band(i)`, `param(i)`, `smooth_pulse()`.
3. Register the name in `src/effects/mod.rs` — add to `EFFECT_NAMES` and `effect_source()`.
4. Rebuild — the scene manager picks it up automatically.

## Dependencies

GPU: wgpu 24 · winit 0.30 · Audio: cpal 0.15 · FFT: rustfft 6 · Text: glyphon 0.8

## License

MIT
