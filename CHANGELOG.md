# Changelog

## Unreleased

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
