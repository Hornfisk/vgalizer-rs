//! Effects enable/disable menu overlay.
//!
//! Toggled with `M`. Lists all registered effects with `[x]`/`[ ]` checkboxes.
//! ↑/↓ moves the cursor, Space toggles, Enter saves (persists to XDG config),
//! Esc cancels and restores the original mask.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

/// Working state for an open effects menu session.
pub struct EffectsMenuState {
    pub effect_names: Vec<String>,
    /// Working enabled mask (parallel to `effect_names`).
    pub enabled: Vec<bool>,
    /// Original mask captured on open, used for cancel/restore.
    pub original: Vec<bool>,
    pub cursor: usize,
}

impl EffectsMenuState {
    pub fn open(effect_names: &[String], current: &[bool]) -> Self {
        // Defensive: pad/clip the incoming mask to match name count.
        let enabled: Vec<bool> = effect_names
            .iter()
            .enumerate()
            .map(|(i, _)| current.get(i).copied().unwrap_or(true))
            .collect();
        Self {
            effect_names: effect_names.to_vec(),
            enabled: enabled.clone(),
            original: enabled,
            cursor: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.effect_names.is_empty() { return; }
        if self.cursor == 0 {
            self.cursor = self.effect_names.len() - 1;
        } else {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.effect_names.is_empty() { return; }
        self.cursor = (self.cursor + 1) % self.effect_names.len();
    }

    /// Toggle the effect under the cursor. Refuses to disable the last
    /// remaining enabled effect (always keeps at least one).
    pub fn toggle_current(&mut self) {
        if self.cursor >= self.enabled.len() { return; }
        let new_state = !self.enabled[self.cursor];
        if !new_state && self.enabled.iter().filter(|b| **b).count() == 1 {
            // Don't allow zero — silently refuse.
            return;
        }
        self.enabled[self.cursor] = new_state;
    }

    /// Names that are currently enabled in the working buffer (for save).
    #[allow(dead_code)]
    pub fn enabled_names(&self) -> Vec<String> {
        self.effect_names
            .iter()
            .zip(self.enabled.iter())
            .filter_map(|(n, &e)| if e { Some(n.clone()) } else { None })
            .collect()
    }

    /// Names the user has turned off — saved as a deny list so new effects
    /// added in future code updates auto-enable.
    pub fn disabled_names(&self) -> Vec<String> {
        self.effect_names
            .iter()
            .zip(self.enabled.iter())
            .filter_map(|(n, &e)| if !e { Some(n.clone()) } else { None })
            .collect()
    }
}

pub struct EffectsMenuOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    /// Cached last text; skip glyphon reshape when unchanged. See T4.
    last_text: String,
}

impl EffectsMenuOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size_px = 20.0;
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(font_size_px, font_size_px * 1.30),
        );
        buffer.set_size(&mut font_system, Some(2400.0), Some(1400.0));

        Self { font_system, swash_cache, atlas, renderer, viewport, buffer, last_text: String::new() }
    }

    pub fn update_text(&mut self, st: &EffectsMenuState, scene_dur: f64) {
        let mut s = String::new();
        let on_count = st.enabled.iter().filter(|b| **b).count();
        s.push_str(&format!(
            "── EFFECTS · {}/{} on · auto {:.0}s ──\n\n",
            on_count, st.effect_names.len(), scene_dur,
        ));

        // Render in two columns when there are many effects, so the whole
        // list fits on one screen even at 26+ entries.
        const COL_WIDTH: usize = 24;
        let n = st.effect_names.len();
        let half = (n + 1) / 2;
        for row in 0..half {
            for col in 0..2 {
                let i = row + col * half;
                if i >= n { continue; }
                let marker = if i == st.cursor { "▶" } else { " " };
                let chk    = if st.enabled[i] { "[x]" } else { "[ ]" };
                let name   = &st.effect_names[i];
                let cell = format!("{} {} {:<width$}", marker, chk, name, width = COL_WIDTH - 6);
                s.push_str(&cell);
                if col == 0 { s.push_str("  "); }
            }
            s.push('\n');
        }

        s.push_str("\n  ↑/↓  move        Space  toggle");
        s.push_str("\n  Shift+↑/↓  auto-time  ±5s");
        s.push_str("\n  Enter save       Esc / M  cancel");

        // Skip glyphon reshape when the body is unchanged between frames.
        if s == self.last_text {
            return;
        }
        self.buffer.set_text(
            &mut self.font_system,
            &s,
            Attrs::new().family(Family::Name("Roboto Condensed")),
            Shaping::Basic,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);
        self.last_text = s;
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
            .expect("Effects menu prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("effects_menu_pass"),
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
                .expect("Effects menu render failed");
        }

        self.atlas.trim();
    }
}
