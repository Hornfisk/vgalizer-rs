# In-app unified `vje` overlay with viz-shrink preview

**Status:** planned 2026-04-08, implementation in progress on pingo.
**Target binary:** `vgalizer` (not the standalone `vje` binary).
**Hotkey:** `V` (free before this patch; mnemonic "vje").

## Context

On pingo (ThinkPad E480, Debian 13, cage + vgalizer on physical tty,
display eDP-1 at **1366×768**), there was no way to do deep param editing
locally without SSHing in from the Arch dev box and running the standalone
`vje` binary. This patch adds that capability directly inside the running
visualizer, with a shrunk preview of the live visuals next to the editor
so the user sees feedback while tweaking — "1 terminal, nothing else",
where "terminal" is the fullscreen vgalizer Wayland surface itself.

## Gap versus the existing in-app overlays

vgalizer already had three separate modal overlays that cover *parts* of
what's needed:

| Key | Overlay | Covers |
|---|---|---|
| `E` | `src/text/params_overlay.rs` | Per-effect params — **current effect only** |
| `M` | `src/effects_menu.rs` | Effects enable/disable (26 effects) |
| `G` | `src/global_settings.rs` | 8 post-fx knobs (`GlobalKnob::ALL`) |

These are kept as quick-access shortcuts. The new `V` overlay is a
*unified* deep editor that closes three gaps:

1. **Cross-effect editing.** Browse all 26 effects and nudge the params of
   any of them, even while a different effect is rendering. Before this,
   `E` was hard-locked to the currently active effect.
2. **Single screen.** Effect list on the left, per-effect params table on
   the right — matches the layout of the standalone `vje` TUI in
   `src/bin/vje/ui.rs`.
3. **Viz-shrink preview.** When the overlay is open, the viz blits into a
   smaller sub-rect instead of covering the whole screen, giving a
   dedicated readable preview region that doesn't fight the text.

## Architecture

New overlay composes existing building blocks:

- `effects::EFFECT_NAMES` + `config.disabled_effects` → effect list
- `effects::params::effect_params()` + direct `state.config.fx_params`
  mutation → per-effect params editing
- `GlobalKnob::ALL` (from `src/global_settings.rs`) → globals tab (8 post-fx
  knobs), plus a handful of additional numeric globals (scene_duration,
  beat_sensitivity, fx_speed_mult)
- `config::write_xdg_fields()` → atomic commit on Enter
- `ConfigWatcher` at `src/app.rs:577-613` → already picks up XDG changes
  within ~100ms, no new wiring needed

Viz-shrink is implemented as a single `pass.set_viewport(x, y, w, h, 0, 1)`
call on the existing blit pass at `src/app.rs:820-837` right before
`pass.draw(0..3, 0..1)`. The effect rendering to the Rgba16Float offscreen
texture stays full-resolution unchanged; only the final blit to swapchain
is scissored. `LoadOp::Clear(BLACK)` at line 826 already paints the
unused surface area black — that becomes the panel background. The
overlay glyphon text then draws on top with `LoadOp::Load`.

Preview rect is **computed proportionally from `state.gpu.size`**, not
hardcoded, so dev on the Arch desktop at 1920×1080+ looks reasonable too:
panel takes the left ~48% of the screen width, preview occupies the right
~48% at 16:9 aspect, centered vertically in the remaining column.

Concrete values at pingo's 1366×768:
- Panel: ~660 px wide × 768 px tall (left-aligned)
- Preview: ~706×397 px 16:9, tucked into the right column vertically
  centered

Operating point ("11×22 font, scrolling list"): font_size_px = 20.0,
line height ~26 px. ~16 effects visible in the list at once; list scrolls
to keep the cursor in view. Bundled monospace font
(`assets/fonts/DejaVuSansMono.ttf`, public domain) so columns line up
— the existing Roboto Condensed is proportional and would break the
grid layout. DejaVu Sans Mono is shipped with Debian and Arch both, so
bundling it via `include_bytes!` just freezes the same glyphs on both
boxes without a runtime font-path lookup.

## Scope of the Globals tab (v1)

Numeric knobs only. Complex global editing (dj_name text, strobe_mode
enum cycling, mirror_count/spread) stays in the standalone vje binary
and the existing T/G modal overlays — it's not the hot path during a
live event.

| Row | Source |
|---|---|
| bleed, chroma, vga_grit, vga_noise, glitch, mirror_a, rot_kick, vibration | `GlobalKnob::ALL` (reused) |
| scene_duration | `state.config.scene_duration` (direct) |
| beat_sensitivity | `state.config.beat_sensitivity` (direct) |
| fx_speed_mult | `state.config.fx_speed_mult` (direct) |

## Files changed

| File | Change |
|---|---|
| `src/text/vje_overlay.rs` | **new** — state + glyphon renderer |
| `src/text/mod.rs` | re-export the new module |
| `src/input.rs` | new `Vje*` actions, modal branch, default `V` binding |
| `src/app.rs` | overlay fields + action handlers + proportional preview rect + `set_viewport` in blit pass + suppress HUD/name when open + render overlay last |
| `src/overlay.rs` | append ` V vje` to HUD hint line |
| `assets/fonts/DejaVuSansMono.ttf` | **new asset** — bundled public-domain monospace font |

## Out of scope

- Not adding ratatui to vgalizer. No custom wgpu ratatui backend.
- Not removing the existing `E` / `M` / `G` overlays — they stay as
  single-key shortcuts.
- Not touching the standalone `vje` binary at `src/bin/vje/`.
- Not changing vgalizer's rendering pipeline except for one
  `set_viewport` call in the blit pass.
- Not adding any new cargo or apt dependencies.

## Verification

1. `cargo build --release` — clean, no new warnings.
2. Iterate from SSH with `./target/release/vgalizer -r 1366x768` (windowed
   on dev box or cage-less on pingo):
   - `V` → overlay appears; viz shrinks to right-side preview; panel fills
     the left with black background and monospace effect list + params.
   - ↑↓ in list; params table updates to highlighted effect's params
     (not the active effect's).
   - Space or Tab switches focus into params; ←/→ nudges (Shift ×10);
     `*` dirty marker appears on edited rows.
   - Enter → commits all dirty rows via `write_xdg_fields`; watcher
     picks up in ~100ms, preview reflects the change live.
   - Tab switches to Globals sub-view; nudge + Enter behaves the same.
   - `X` on an effect toggles its disabled marker; Enter commits the
     `disabled_effects` deny list.
   - V or Escape closes the overlay; HUD + name overlay return.
3. `cat ~/.config/vgalizer/config.json` to confirm atomic commits landed.
4. Full-stack: `cage -- ./target/release/vgalizer` on the tty, test with
   the physical keyboard from ~1 m seated distance.
