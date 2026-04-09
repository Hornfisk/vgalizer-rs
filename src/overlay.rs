/// Basic HUD overlay: effect name, BPM, sensitivity, key hints.
/// Renders as simple text in the top-left corner using glyphon.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

pub struct HudOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    visible: bool,
    /// Last string handed to glyphon. `update_text` skips
    /// `set_text` + `shape_until_scroll` when the newly-built string is
    /// identical, eliminating per-frame reshape thrashing on unchanged
    /// HUD content. See T4 in the debug plan.
    last_text: String,
}

impl HudOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size = 18.0;
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, font_size * 1.4));
        // Wide enough that the shortcuts line never wraps; bounds.right clips at render time.
        buffer.set_size(&mut font_system, Some(3000.0), Some(200.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            visible: true,
            last_text: String::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn update_text(
        &mut self,
        effect: &str,
        bpm: f32,
        bpm_locked: bool,
        sensitivity: f32,
        level: f32,
        scene_dur: f64,
        stats_line: &str,
    ) {
        let bar = level_bar(level);
        // BPM readout: "128*" when the tempo tracker is locked, "~128"
        // while still tracking the pre-lock estimate. The star is ASCII
        // so the bundled Roboto Condensed Bold renders it without
        // falling back to a tofu glyph.
        let bpm_str = if bpm_locked {
            format!("{:.0}*", bpm)
        } else {
            format!("~{:.0}", bpm)
        };
        // Three lines when stats are present: effect / stats / shortcuts.
        // The middle line is the T5 HUD extension — FPS, CPU%, temps,
        // RAM, GPU freq. Empty string skips the middle line entirely.
        let text = if stats_line.is_empty() {
            format!(
                "Effect: {}  BPM: {}  Sens: {:.1}  Auto: {:.0}s  Lvl: {}\nSPACE next  1-9 jump  ↑↓ sens  Shift+↑↓ auto  P mirror  A device  T name  E params  M effects  G global  V vje  H hide  Q quit",
                effect, bpm_str, sensitivity, scene_dur, bar
            )
        } else {
            format!(
                "Effect: {}  BPM: {}  Sens: {:.1}  Auto: {:.0}s  Lvl: {}\n{}\nSPACE next  1-9 jump  ↑↓ sens  Shift+↑↓ auto  P mirror  A device  T name  E params  M effects  G global  V vje  H hide  Q quit",
                effect, bpm_str, sensitivity, scene_dur, bar, stats_line
            )
        };
        // Skip the glyphon reshape if the string is byte-identical to the
        // last one. Avoids ~60 reshapes/sec of unchanged HUD content when
        // effect/bpm/sens/level/scene_dur haven't changed between frames.
        if text == self.last_text {
            return;
        }
        self.buffer.set_text(
            &mut self.font_system,
            &text,
            Attrs::new().family(Family::Name("Roboto Condensed")),
            Shaping::Basic,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
        self.last_text = text;
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        screen_size: (u32, u32),
    ) {
        if !self.visible { return; }

        self.viewport.update(queue, Resolution {
            width: screen_size.0,
            height: screen_size.1,
        });

        let margin = 20i32;
        let areas = [TextArea {
            buffer: &self.buffer,
            left: margin as f32,
            top: 12.0,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: screen_size.0 as i32 - margin,
                bottom: 150,
            },
            default_color: Color::rgba(200, 200, 200, 200),
            custom_glyphs: &[],
        }];

        self.renderer
            .prepare(device, queue, &mut self.font_system, &mut self.atlas, &self.viewport, areas, &mut self.swash_cache)
            .expect("HUD prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud_pass"),
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
                .expect("HUD render failed");
        }

        self.atlas.trim();
    }
}

/// 8-char ASCII level bar, e.g. "####----".
/// Uses pure ASCII so any font renders it correctly (the bundled Roboto
/// Condensed Bold does not include the Unicode block-drawing glyphs).
fn level_bar(level: f32) -> String {
    let filled = (level.clamp(0.0, 1.0) * 8.0).round() as usize;
    "#".repeat(filled) + &"-".repeat(8 - filled)
}
