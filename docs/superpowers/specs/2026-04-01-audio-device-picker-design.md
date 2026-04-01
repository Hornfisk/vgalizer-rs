# Audio Device Picker — Design Spec
_Date: 2026-04-01_

## Problem

vgalizer-rs reads audio from whichever input device cpal selects by default (or one hard-coded in config), but the correct device isn't always obvious — especially with virtual sinks, USB interfaces, and loopback devices all present. There is no in-app way to switch devices without editing config.json and restarting.

## Goal

Let the user open an in-app audio device picker with `A`, see live signal bars for each device so they can identify the active one, select it with arrow keys or number keys + Enter, and have the choice persist to config.

---

## Architecture

### New module: `src/audio_picker.rs`

Two responsibilities:

**`AudioSignalScanner`** — background thread, spawned when picker opens.
- Enumerates all cpal input devices.
- For each device, attempts to open a brief cpal stream and record RMS level over ~200 ms.
- Sends `(device_index, rms: f32)` updates via `mpsc::channel` back to the render thread.
- Streams are held open while the picker is visible; dropped on close.
- Devices that fail to open silently show 0.0 signal.

**`AudioPickerOverlay`** — renders the picker panel using the existing `glyphon` text pipeline.
- Holds device list, selected index, current signal levels.
- `update(rx: &Receiver<(usize, f32)>)` drains the channel each frame.
- `render(...)` draws the panel using the same `TextRenderer` / `TextAtlas` pattern as `overlay.rs`.

### State in `AppState` (`src/app.rs`)

Add field:
```rust
audio_picker: Option<AudioPickerState>
// None = closed; Some = open with devices + scanner handle
```

On `A` key press: if closed → enumerate devices, spawn scanner, set `Some(...)`. If open → drop scanner, set `None`.

On `Enter` key (when picker open):
1. Stop current cpal stream (send shutdown signal to capture thread).
2. Call `start_capture(Some(device_name), audio_state)` with selected device.
3. Write `audio_device` field to `~/.config/vgalizer/config.json` via new config helper.
4. Close picker.

---

## UI Layout

Panel rendered top-left below the HUD text, using same Roboto Condensed font, dark semi-transparent background rect.

```
Audio Device  [Esc to cancel]
────────────────────────────────
  1  default                ░░░░
► 2  HD Pro Webcam          ████
  3  USB Audio              ░░
  4  Monitor of Built-in    ░░░░
────────────────────────────────
↑↓ navigate   Enter select
```

- `►` marks selected row; updates on `↑`/`↓` or `1`–`9` press.
- Signal bar: 8-character wide, filled proportionally to RMS (clamped 0–1). Chars: `░` (empty) / `█` (filled).
- Currently active device is pre-selected when picker opens.
- Max 9 devices shown (numbered `1`–`9`); if more exist, show first 9.

HUD shortcuts line (in `overlay.rs`) gains `A device` before `H hide`.

---

## Input Routing (`src/input.rs`)

New action: `Action::ToggleAudioPicker`  → physical key `A`.

When picker is open, additional actions consumed by the picker:
- `ArrowUp` / `ArrowDown` → move selection
- `1`–`9` → jump to that device
- `Enter` → confirm selection
- `Escape` → cancel (also bound to existing Quit only when picker is closed)

All other shortcuts (`Space`, `+`/`-`, `P`, `H`, `F`, etc.) remain active while picker is open.

---

## Config Persistence (`src/config/mod.rs`)

New helper: `write_audio_device(path: &Path, device_name: &str) -> Result<()>`
- Reads current config JSON (or uses `{}` if file doesn't exist).
- Sets `audio_device` field.
- Writes back atomically (write to `.tmp` then rename).
- The hot-reload watcher debounce (100 ms) naturally absorbs the write without triggering a spurious reload.

Config path used: same priority order as existing load logic — XDG path `~/.config/vgalizer/config.json` if it exists, else repo-local `config.json`.

---

## Files Changed

| File | Change |
|------|--------|
| `src/audio_picker.rs` (new) | `AudioPickerOverlay` + `AudioSignalScanner` |
| `src/app.rs` | `audio_picker` field; open/close/select logic; render call in frame loop |
| `src/input.rs` | `ToggleAudioPicker` action; picker-aware input routing |
| `src/overlay.rs` | Add `A device` to shortcuts hint text |
| `src/config/mod.rs` | Add `write_audio_device` helper |
| `src/lib.rs` | Declare `mod audio_picker` |

---

## Verification

1. Run `cargo build` — zero warnings.
2. Launch app. HUD shows `A device` in shortcuts line.
3. Press `H` — shortcuts line disappears; `A device` gone too. Press `H` again — returns.
4. Press `A` — picker opens with device list and live signal bars.
5. Play audio on one device — its bar fills.
6. Navigate with `↑`/`↓`, press `Enter` — picker closes, audio switches, BPM/level starts responding.
7. Check `~/.config/vgalizer/config.json` — `audio_device` field set to selected device name.
8. Restart app — same device is used automatically.
9. Press `A` then `Esc` — picker closes, device unchanged.
