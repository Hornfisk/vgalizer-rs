/// In-app audio device picker.
///
/// Three layers:
///   AudioPickerState  — pure selection state (no GPU, no threads; fully testable)
///   SignalScanner     — background thread that opens cpal streams on all devices
///   AudioPicker       — combines state + scanner; lives in AppState when picker is open
///   AudioPickerOverlay — glyphon text renderer; always resident in AppState
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

// ─── Pure picker state ────────────────────────────────────────────────────────

pub struct AudioPickerState {
    pub devices: Vec<String>,
    pub selected: usize,
    pub signal_levels: Vec<f32>,
}

impl AudioPickerState {
    pub fn new(devices: Vec<String>, active_device: Option<&str>) -> Self {
        let selected = active_device
            .and_then(|name| {
                let lower = name.to_lowercase();
                devices
                    .iter()
                    .position(|d| d.to_lowercase().contains(&lower))
            })
            .unwrap_or(0);
        let n = devices.len();
        Self {
            devices,
            selected,
            signal_levels: vec![0.0; n],
        }
    }

    pub fn move_up(&mut self) {
        if self.devices.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.devices.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.devices.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.devices.len();
    }

    /// `n` is 1-indexed (matching the on-screen numbers 1–9).
    pub fn jump_to_1indexed(&mut self, n: usize) {
        if n >= 1 && n <= self.devices.len() {
            self.selected = n - 1;
        }
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.devices.get(self.selected).map(|s| s.as_str())
    }

    pub fn update_levels(&mut self, levels: &[f32]) {
        for (i, &lvl) in levels.iter().enumerate() {
            if i < self.signal_levels.len() {
                self.signal_levels[i] = lvl;
            }
        }
    }
}

// ─── Text formatting helpers ──────────────────────────────────────────────────

/// Returns an 8-character signal bar string (e.g. "████░░░░").
pub fn format_signal_bar(level: f32) -> String {
    let filled = (level.clamp(0.0, 1.0) * 8.0).round() as usize;
    let empty = 8usize.saturating_sub(filled);
    "█".repeat(filled) + &"░".repeat(empty)
}

/// Renders the full picker overlay text from current state.
pub fn format_picker_text(state: &AudioPickerState) -> String {
    const WIDTH: usize = 36;
    let divider = "─".repeat(WIDTH);
    let mut lines = vec![
        "Audio Device  [Esc to close]".to_string(),
        divider.clone(),
    ];
    for (i, name) in state.devices.iter().enumerate().take(9) {
        let cursor = if i == state.selected { "►" } else { " " };
        let alsa_level = state.signal_levels.get(i).copied().unwrap_or(0.0);
        let (display, bar) = if let Some(src) = name.strip_prefix("PA:") {
            (pa_display_name(src), "████████".to_string())
        } else if let Some(src) = name.strip_prefix("pa:") {
            (pa_display_name(src), "░░░░░░░░".to_string())
        } else if let Some(src) = name.strip_prefix("PW:") {
            (pa_display_name(src), "████████".to_string())
        } else if let Some(src) = name.strip_prefix("pw:") {
            (pa_display_name(src), "░░░░░░░░".to_string())
        } else {
            let d: String = name.chars().take(20).collect();
            (d, format_signal_bar(alsa_level))
        };
        lines.push(format!("{} {}  {:<20}  {}", cursor, i + 1, display, bar));
    }
    lines.push(divider);
    lines.push("↑↓ navigate   Enter select".to_string());
    lines.join("\n")
}

/// Extracts a short human-readable profile name from a PulseAudio source name.
/// e.g. `alsa_output.usb-DJControl.analog-surround-40.monitor` → `♪ analog-surround-40`
fn pa_display_name(source_name: &str) -> String {
    let base = source_name.strip_suffix(".monitor").unwrap_or(source_name);
    let profile = base.rsplit('.').next().unwrap_or(base);
    let display: String = format!("♪ {}", profile).chars().take(20).collect();
    display
}

// ─── Signal scanner ───────────────────────────────────────────────────────────

/// Background thread that opens a cpal input stream for each named device and
/// measures running RMS levels. Devices are matched by exact name so the
/// levels Vec index always corresponds to the same position as the picker list.
pub struct SignalScanner {
    pub levels: Arc<Mutex<Vec<f32>>>,
    stop: Arc<AtomicBool>,
    _handle: std::thread::JoinHandle<()>,
}

impl SignalScanner {
    /// Spawns the scanner for the given device names. Index `i` in `levels`
    /// corresponds to `device_names[i]`.
    pub fn start(device_names: Vec<String>) -> Self {
        let n = device_names.len();
        let levels = Arc::new(Mutex::new(vec![0.0f32; n]));
        let stop = Arc::new(AtomicBool::new(false));

        let levels_thread = levels.clone();
        let stop_thread = stop.clone();

        let handle = std::thread::spawn(move || {
            run_scanner(device_names, levels_thread, stop_thread);
        });

        Self {
            levels,
            stop,
            _handle: handle,
        }
    }
}

impl Drop for SignalScanner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run_scanner(device_names: Vec<String>, levels: Arc<Mutex<Vec<f32>>>, stop: Arc<AtomicBool>) {
    let host = cpal::default_host();

    // Build a map: device name → cpal Device
    let device_map: std::collections::HashMap<String, cpal::Device> = match host.input_devices() {
        Ok(it) => it
            .filter_map(|d| d.name().ok().map(|n| (n, d)))
            .collect(),
        Err(_) => return,
    };

    // Open a stream for each named device; indexed to match picker list.
    let _streams: Vec<Option<cpal::Stream>> = device_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let dev = device_map.get(name)?;
            let config = dev.default_input_config().ok()?.into();
            let lvls = levels.clone();
            let stream = dev
                .build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if data.is_empty() {
                            return;
                        }
                        let rms =
                            (data.iter().map(|x| x * x).sum::<f32>() / data.len() as f32).sqrt();
                        if let Ok(mut v) = lvls.lock() {
                            if i < v.len() {
                                v[i] = v[i] * 0.85 + rms * 0.15;
                            }
                        }
                    },
                    |e| log::warn!("Scanner stream error: {}", e),
                    None,
                )
                .ok()?;
            stream.play().ok()?;
            Some(stream)
        })
        .collect();

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    // _streams dropped here, all cpal streams stop
}

// ─── Combined picker (state + scanner) ───────────────────────────────────────

pub struct AudioPicker {
    pub state: AudioPickerState,
    scanner: SignalScanner,
}

impl AudioPicker {
    /// Build picker from the filtered device list; pre-selects `active_device` if found.
    pub fn open(active_device: Option<&str>) -> Self {
        let names: Vec<String> = crate::audio::capture::list_input_devices_for_picker()
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        let state = AudioPickerState::new(names.clone(), active_device);
        let scanner = SignalScanner::start(names);
        Self { state, scanner }
    }

    /// Drain latest signal levels from scanner into state. Call once per frame.
    pub fn tick(&mut self) {
        if let Ok(lvls) = self.scanner.levels.lock() {
            self.state.update_levels(&lvls);
        }
    }
}

// ─── GPU overlay renderer ─────────────────────────────────────────────────────

pub struct AudioPickerOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
}

impl AudioPickerOverlay {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size = 17.0;
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, font_size * 1.4));
        buffer.set_size(&mut font_system, Some(520.0), Some(400.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
        }
    }

    pub fn update(&mut self, picker: &AudioPicker) {
        let text = format_picker_text(&picker.state);
        self.buffer.set_text(
            &mut self.font_system,
            &text,
            Attrs::new().family(Family::Name("Roboto Condensed")),
            Shaping::Basic,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        screen_size: (u32, u32),
    ) {
        self.viewport.update(
            queue,
            Resolution {
                width: screen_size.0,
                height: screen_size.1,
            },
        );

        let areas = [TextArea {
            buffer: &self.buffer,
            left: 12.0,
            top: 72.0, // below the HUD
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: 600,
                bottom: 500,
            },
            default_color: Color::rgba(230, 230, 230, 230),
            custom_glyphs: &[],
        }];

        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            )
            .expect("AudioPickerOverlay prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("audio_picker_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .expect("AudioPickerOverlay render failed");
        }

        self.atlas.trim();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // AudioPickerState::new

    #[test]
    fn picker_preselects_matching_active_device() {
        let devices = vec![
            "Built-in Microphone".to_string(),
            "USB Audio Interface".to_string(),
            "HD Webcam".to_string(),
        ];
        let picker = AudioPickerState::new(devices, Some("USB Audio"));
        assert_eq!(picker.selected, 1);
    }

    #[test]
    fn picker_preselect_is_case_insensitive() {
        let devices = vec!["Built-in Microphone".to_string(), "USB Audio".to_string()];
        let picker = AudioPickerState::new(devices, Some("usb audio"));
        assert_eq!(picker.selected, 1);
    }

    #[test]
    fn picker_defaults_to_zero_with_no_active_device() {
        let devices = vec!["Mic A".to_string(), "Mic B".to_string()];
        let picker = AudioPickerState::new(devices, None);
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn picker_defaults_to_zero_when_active_device_not_found() {
        let devices = vec!["Mic A".to_string(), "Mic B".to_string()];
        let picker = AudioPickerState::new(devices, Some("Phantom Device"));
        assert_eq!(picker.selected, 0);
    }

    // move_down

    #[test]
    fn move_down_advances_selection() {
        let devices = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.move_down();
        assert_eq!(picker.selected, 1);
    }

    #[test]
    fn move_down_wraps_from_last_to_first() {
        let devices = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.selected = 2;
        picker.move_down();
        assert_eq!(picker.selected, 0);
    }

    // move_up

    #[test]
    fn move_up_decrements_selection() {
        let devices = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.selected = 2;
        picker.move_up();
        assert_eq!(picker.selected, 1);
    }

    #[test]
    fn move_up_wraps_from_first_to_last() {
        let devices = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.move_up();
        assert_eq!(picker.selected, 2);
    }

    // jump_to_1indexed

    #[test]
    fn jump_to_sets_correct_index() {
        let devices = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.jump_to_1indexed(3);
        assert_eq!(picker.selected, 2);
    }

    #[test]
    fn jump_to_ignores_zero() {
        let devices = vec!["A".to_string(), "B".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.selected = 1;
        picker.jump_to_1indexed(0);
        assert_eq!(picker.selected, 1, "zero is not a valid 1-indexed position");
    }

    #[test]
    fn jump_to_ignores_out_of_range() {
        let devices = vec!["A".to_string(), "B".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.jump_to_1indexed(10);
        assert_eq!(picker.selected, 0, "out-of-range jump should leave selection unchanged");
    }

    // selected_name

    #[test]
    fn selected_name_returns_current_device() {
        let devices = vec!["First".to_string(), "Second".to_string()];
        let mut picker = AudioPickerState::new(devices, None);
        picker.selected = 1;
        assert_eq!(picker.selected_name(), Some("Second"));
    }

    // format_signal_bar

    #[test]
    fn format_bar_empty_at_zero() {
        assert_eq!(format_signal_bar(0.0), "░░░░░░░░");
    }

    #[test]
    fn format_bar_full_at_one() {
        assert_eq!(format_signal_bar(1.0), "████████");
    }

    #[test]
    fn format_bar_half_at_point_five() {
        assert_eq!(format_signal_bar(0.5), "████░░░░");
    }

    #[test]
    fn format_bar_clamps_above_one() {
        assert_eq!(format_signal_bar(2.0), "████████");
    }

    #[test]
    fn format_bar_clamps_below_zero() {
        assert_eq!(format_signal_bar(-1.0), "░░░░░░░░");
    }

    // format_picker_text

    #[test]
    fn format_picker_text_marks_selected_with_cursor() {
        let devices = vec!["Mic A".to_string(), "Mic B".to_string()];
        let picker = AudioPickerState::new(devices, None);
        let text = format_picker_text(&picker);
        let lines: Vec<&str> = text.lines().collect();
        // line[2] is the first device entry (after header + divider)
        assert!(lines[2].contains('►'), "selected row should show ►");
        assert!(!lines[3].contains('►'), "non-selected row must not show ►");
    }

    #[test]
    fn format_picker_text_shows_device_numbers() {
        let devices = vec!["Mic A".to_string(), "Mic B".to_string(), "Mic C".to_string()];
        let picker = AudioPickerState::new(devices, None);
        let text = format_picker_text(&picker);
        assert!(text.contains(" 1 "), "should show device number 1");
        assert!(text.contains(" 2 "), "should show device number 2");
        assert!(text.contains(" 3 "), "should show device number 3");
    }

    #[test]
    fn format_picker_text_shows_signal_bars() {
        let mut picker = AudioPickerState::new(vec!["Mic".to_string()], None);
        picker.signal_levels[0] = 1.0;
        let text = format_picker_text(&picker);
        assert!(text.contains("████████"), "full signal should show full bar");
    }

    #[test]
    fn format_picker_text_caps_at_nine_devices() {
        let devices: Vec<String> = (1..=12).map(|i| format!("Device {}", i)).collect();
        let picker = AudioPickerState::new(devices, None);
        let text = format_picker_text(&picker);
        // 12 devices but only 9 shown: header(1) + divider(1) + entries(9) + divider(1) + footer(1) = 13 lines
        assert_eq!(text.lines().count(), 13);
    }
}
