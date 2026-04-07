//! Live per-effect parameter editor overlay.
//! Toggled with `E`. ↑/↓ select param, ←/→ nudge by `step`
//! (Shift = ×10), Enter persists to `config.json`, Esc cancels.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::effects::params::{effect_params, ParamDef};

/// In-memory state of an open editor session for one effect.
pub struct ParamEditState {
    pub effect: String,
    pub defs: &'static [ParamDef],
    /// Working values (unsaved); index parallels `defs`.
    pub values: Vec<f32>,
    /// Original values when the editor was opened (for cancel/restore).
    pub original: Vec<f32>,
    /// Currently highlighted row.
    pub cursor: usize,
}

impl ParamEditState {
    /// Open the editor for `effect`, seeding values from the supplied
    /// `EffectUniforms.params` array.
    pub fn open(effect: &str, current_params: &[f32; 16]) -> Option<Self> {
        let defs = effect_params(effect);
        if defs.is_empty() {
            return None;
        }
        let values: Vec<f32> = defs
            .iter()
            .enumerate()
            .map(|(i, d)| current_params[i].clamp(d.min, d.max))
            .collect();
        Some(Self {
            effect: effect.to_string(),
            defs,
            values: values.clone(),
            original: values,
            cursor: 0,
        })
    }

    pub fn select_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        } else {
            self.cursor = self.defs.len().saturating_sub(1);
        }
    }
    pub fn select_down(&mut self) {
        self.cursor = (self.cursor + 1) % self.defs.len().max(1);
    }
    pub fn nudge(&mut self, dir: i32, fast: bool) {
        if self.cursor >= self.defs.len() { return; }
        let def = &self.defs[self.cursor];
        let mult = if fast { 10.0 } else { 1.0 };
        let nv = (self.values[self.cursor] + dir as f32 * def.step * mult)
            .clamp(def.min, def.max);
        self.values[self.cursor] = nv;
    }

    /// Restore the original (entry-time) values into the working buffer
    /// so the caller can re-upload them on cancel.
    pub fn restore_original(&mut self) {
        self.values = self.original.clone();
    }

    /// Pack the current working values into a 16-slot params array.
    pub fn as_params_array(&self) -> [f32; 16] {
        let mut a = [0.0f32; 16];
        for (i, v) in self.values.iter().enumerate() {
            if i < 16 { a[i] = *v; }
        }
        a
    }
}

pub struct ParamsOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    font_size_px: f32,
}

impl ParamsOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../../assets/fonts/RobotoCondensed-Bold.ttf");
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

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            font_size_px,
        }
    }

    pub fn update_text(&mut self, st: &ParamEditState) {
        let mut s = String::new();
        s.push_str(&format!("─ {} ─\n", st.effect));
        for (i, def) in st.defs.iter().enumerate() {
            let marker = if i == st.cursor { ">" } else { " " };
            // 12-step bar showing position in [min..max]
            let frac = ((st.values[i] - def.min) / (def.max - def.min)).clamp(0.0, 1.0);
            let filled = (frac * 12.0).round() as usize;
            let bar: String = (0..12)
                .map(|c| if c < filled { '█' } else { '·' })
                .collect();
            s.push_str(&format!(
                "{} {:<11}  {}  {:.2}\n",
                marker, def.name, bar, st.values[i]
            ));
        }
        s.push_str("\n↑↓ select   ←→ nudge (Shift ×10)   Enter save   Esc cancel");

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

        // Place top-left, indented from screen edge.
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
            .expect("Params overlay prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("params_overlay_pass"),
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
                .expect("Params overlay render failed");
        }

        self.atlas.trim();
    }

    #[allow(dead_code)]
    pub fn font_px(&self) -> f32 { self.font_size_px }
}
