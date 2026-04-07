//! Global post-processing settings overlay (`G`).
//!
//! Lets the user dial down the global knobs that affect every effect:
//! trail bleed/glow, chromatic aberration, VGA grunge, glitch tear, mirror
//! overlay, beat rotation/vibration. Mutates `state.config` directly so
//! the change is visible the next frame, and persists to the XDG config on
//! `Enter` so it survives rebuilds.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::config::Config;

/// One adjustable global knob. Each variant knows how to read/write the
/// underlying `Config` field as a normalized 0..1 value, plus the JSON key
/// to persist under and a human label for the UI.
#[derive(Debug, Clone, Copy)]
pub enum GlobalKnob {
    Bleed,
    Chroma,
    VgaGrit,
    VgaNoise,
    Glitch,
    MirrorAlpha,
    RotKick,
    Vibration,
}

impl GlobalKnob {
    pub const ALL: &'static [GlobalKnob] = &[
        Self::Bleed,
        Self::Chroma,
        Self::VgaGrit,
        Self::VgaNoise,
        Self::Glitch,
        Self::MirrorAlpha,
        Self::RotKick,
        Self::Vibration,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bleed       => "bleed",
            Self::Chroma      => "chroma",
            Self::VgaGrit     => "vga_grit",
            Self::VgaNoise    => "vga_noise",
            Self::Glitch      => "glitch",
            Self::MirrorAlpha => "mirror_a",
            Self::RotKick     => "rot_kick",
            Self::Vibration   => "vibration",
        }
    }

    /// JSON key under which this knob is persisted in the XDG config.
    pub fn config_key(&self) -> &'static str {
        match self {
            Self::Bleed       => "trail_alpha",
            Self::Chroma      => "vga_ca",
            Self::VgaGrit     => "vga_intensity",
            Self::VgaNoise    => "vga_noise",
            Self::Glitch      => "glitch_intensity",
            Self::MirrorAlpha => "mirror_alpha",
            Self::RotKick     => "global_rotation",
            Self::Vibration   => "global_vibration",
        }
    }

    pub fn step(&self) -> f32 { 0.05 }

    /// Read the current value of this knob from the config, normalized to
    /// 0..1. `bleed` is *inverted* — high trail_alpha = clean image = low
    /// bleed — so the slider matches user intuition.
    pub fn read(&self, c: &Config) -> f32 {
        match self {
            Self::Bleed       => 1.0 - (c.trail_alpha as f32 / 255.0).clamp(0.0, 1.0),
            Self::Chroma      => (c.vga_ca as f32 / 16.0).clamp(0.0, 1.0),
            Self::VgaGrit     => c.vga_intensity.clamp(0.0, 1.0),
            Self::VgaNoise    => c.vga_noise.clamp(0.0, 1.0),
            Self::Glitch      => c.glitch_intensity.clamp(0.0, 1.0),
            Self::MirrorAlpha => (c.mirror_alpha as f32 / 255.0).clamp(0.0, 1.0),
            Self::RotKick     => c.global_rotation.clamp(0.0, 1.0),
            Self::Vibration   => c.global_vibration.clamp(0.0, 1.0),
        }
    }

    /// Write a normalized 0..1 value back into the config field, mapping
    /// to whatever physical range / type the field uses.
    pub fn write(&self, c: &mut Config, v: f32) {
        let v = v.clamp(0.0, 1.0);
        match self {
            Self::Bleed       => c.trail_alpha = ((1.0 - v) * 255.0).round() as u32,
            Self::Chroma      => c.vga_ca = (v * 16.0).round() as u32,
            Self::VgaGrit     => c.vga_intensity = v,
            Self::VgaNoise    => c.vga_noise = v,
            Self::Glitch      => c.glitch_intensity = v,
            Self::MirrorAlpha => c.mirror_alpha = (v * 255.0).round() as u32,
            Self::RotKick     => c.global_rotation = v,
            Self::Vibration   => c.global_vibration = v,
        }
    }

    /// Serialize the *current* config field as a JSON value, used for
    /// persistence when the user hits Enter.
    pub fn to_json(&self, c: &Config) -> serde_json::Value {
        match self {
            Self::Bleed       => serde_json::json!(c.trail_alpha),
            Self::Chroma      => serde_json::json!(c.vga_ca),
            Self::VgaGrit     => serde_json::json!(c.vga_intensity),
            Self::VgaNoise    => serde_json::json!(c.vga_noise),
            Self::Glitch      => serde_json::json!(c.glitch_intensity),
            Self::MirrorAlpha => serde_json::json!(c.mirror_alpha),
            Self::RotKick     => serde_json::json!(c.global_rotation),
            Self::Vibration   => serde_json::json!(c.global_vibration),
        }
    }
}

/// Open editor session. Holds a snapshot of the original values so Esc
/// can revert without persisting.
pub struct GlobalSettingsState {
    pub cursor: usize,
    /// Snapshot of values at open-time, in the same order as `GlobalKnob::ALL`.
    pub original: Vec<f32>,
}

impl GlobalSettingsState {
    pub fn open(c: &Config) -> Self {
        let original = GlobalKnob::ALL.iter().map(|k| k.read(c)).collect();
        Self { cursor: 0, original }
    }

    pub fn select_up(&mut self) {
        let n = GlobalKnob::ALL.len();
        if n == 0 { return; }
        self.cursor = if self.cursor == 0 { n - 1 } else { self.cursor - 1 };
    }

    pub fn select_down(&mut self) {
        let n = GlobalKnob::ALL.len();
        if n == 0 { return; }
        self.cursor = (self.cursor + 1) % n;
    }

    pub fn nudge(&mut self, c: &mut Config, dir: i32, fast: bool) {
        if self.cursor >= GlobalKnob::ALL.len() { return; }
        let knob = GlobalKnob::ALL[self.cursor];
        let mult = if fast { 10.0 } else { 1.0 };
        let cur = knob.read(c);
        let nv = (cur + dir as f32 * knob.step() * mult).clamp(0.0, 1.0);
        knob.write(c, nv);
    }

    /// Restore the snapshot taken at open-time into the config (cancel).
    pub fn restore(&self, c: &mut Config) {
        for (i, knob) in GlobalKnob::ALL.iter().enumerate() {
            if let Some(&v) = self.original.get(i) {
                knob.write(c, v);
            }
        }
    }
}

pub struct GlobalSettingsOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
}

impl GlobalSettingsOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size_px = 24.0;
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(font_size_px, font_size_px * 1.30),
        );
        buffer.set_size(&mut font_system, Some(2200.0), Some(900.0));

        Self { font_system, swash_cache, atlas, renderer, viewport, buffer }
    }

    pub fn update_text(&mut self, st: &GlobalSettingsState, c: &Config) {
        const BAR_W: usize = 16;
        let mut s = String::new();
        s.push_str("── GLOBAL SETTINGS ──\n\n");

        for (i, knob) in GlobalKnob::ALL.iter().enumerate() {
            let cur = knob.read(c);
            let marker = if i == st.cursor { "▶" } else { " " };
            let filled = (cur * BAR_W as f32).round() as usize;
            let bar: String = (0..BAR_W)
                .map(|c| if c < filled { '█' } else { '·' })
                .collect();
            let pct = (cur * 100.0).round() as i32;
            let dirty = if (cur - st.original.get(i).copied().unwrap_or(cur)).abs() > 1e-4 {
                "*"
            } else {
                " "
            };
            s.push_str(&format!(
                "{} {} {:<10}  {}  {:>3}%\n",
                marker, dirty, knob.label(), bar, pct
            ));
        }

        s.push_str("\n  ↑/↓  select");
        s.push_str("\n  ←/→  nudge       (Shift  ×10 step)");
        s.push_str("\n  Enter save        Esc / G  cancel");
        s.push_str("\n  *    = unsaved change");

        self.buffer.set_text(
            &mut self.font_system,
            &s,
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
        self.viewport.update(queue, Resolution {
            width: screen_size.0,
            height: screen_size.1,
        });

        let left = 40.0;
        let top  = 80.0;

        let areas = [TextArea {
            buffer: &self.buffer,
            left,
            top,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: screen_size.0 as i32,
                bottom: screen_size.1 as i32,
            },
            default_color: Color::rgba(255, 255, 255, 240),
            custom_glyphs: &[],
        }];

        self.renderer
            .prepare(
                device, queue, &mut self.font_system, &mut self.atlas,
                &self.viewport, areas, &mut self.swash_cache,
            )
            .expect("Global settings prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("global_settings_pass"),
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
                .expect("Global settings render failed");
        }

        self.atlas.trim();
    }
}
