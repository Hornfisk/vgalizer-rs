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
    /// `EffectUniforms.params` array. Returns `Some` even when the effect
    /// has no editable params — the overlay then renders an "(no editable
    /// parameters)" info screen so the `E` keypress always has feedback.
    pub fn open(effect: &str, current_params: &[f32; 16]) -> Self {
        let defs = effect_params(effect);
        let values: Vec<f32> = defs
            .iter()
            .enumerate()
            .map(|(i, d)| current_params[i].clamp(d.min, d.max))
            .collect();
        // Start the cursor on the first non-(unused) row so the first
        // arrow keypress lands somewhere visible.
        let cursor = defs
            .iter()
            .position(|d| d.name != "(unused)")
            .unwrap_or(0);
        Self {
            effect: effect.to_string(),
            defs,
            values: values.clone(),
            original: values,
            cursor,
        }
    }

    /// Whether this editor session has any *visible* editable parameters
    /// (i.e. at least one def whose name isn't `(unused)`).
    pub fn has_params(&self) -> bool {
        self.visible_indices().next().is_some()
    }

    /// Iterator over indices of defs that should be shown in the UI and
    /// reachable by the cursor. Filters out the `(unused)` placeholder
    /// rows that exist only to keep `params[i]` aligned with the shader's
    /// `param(Nu)` slots.
    pub fn visible_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.name != "(unused)")
            .map(|(i, _)| i)
    }

    pub fn select_up(&mut self) {
        let visible: Vec<usize> = self.visible_indices().collect();
        if visible.is_empty() { return; }
        let pos = visible.iter().position(|&i| i == self.cursor).unwrap_or(0);
        let next = if pos == 0 { visible.len() - 1 } else { pos - 1 };
        self.cursor = visible[next];
    }
    pub fn select_down(&mut self) {
        let visible: Vec<usize> = self.visible_indices().collect();
        if visible.is_empty() { return; }
        let pos = visible.iter().position(|&i| i == self.cursor).unwrap_or(0);
        let next = (pos + 1) % visible.len();
        self.cursor = visible[next];
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
    /// Cached last text; `update_text` skips glyphon reshape when the
    /// newly-built body is identical. See T4 in the debug plan.
    last_text: String,
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
            last_text: String::new(),
        }
    }

    pub fn update_text(&mut self, st: &ParamEditState) {
        const BAR_W: usize = 16;
        let mut s = String::new();

        // Header
        s.push_str(&format!("── PARAMS · {} ──\n\n", st.effect));

        let visible: Vec<usize> = st.visible_indices().collect();
        if visible.is_empty() {
            // Info mode: effect has no tweakable params at all (e.g.
            // mandelbrot_zoom). Still show the overlay so `E` has visible
            // feedback, plus the dismiss key.
            s.push_str("  (this effect has no editable parameters)\n");
            s.push_str("\n  Esc / E   close\n");
        } else {
            // Param list — only the user-facing rows; (unused) shader
            // slots are silently kept in the array for index alignment.
            for &i in &visible {
                let def = &st.defs[i];
                let marker = if i == st.cursor { "▶" } else { " " };
                let frac = ((st.values[i] - def.min) / (def.max - def.min)).clamp(0.0, 1.0);
                let filled = (frac * BAR_W as f32).round() as usize;
                let bar: String = (0..BAR_W)
                    .map(|c| if c < filled { '█' } else { '·' })
                    .collect();
                let pct = (frac * 100.0).round() as i32;
                let dirty = if (st.values[i] - st.original[i]).abs() > 1e-6 { "*" } else { " " };
                s.push_str(&format!(
                    "{} {} {:<11}  {}  {:>3}%   {:.2}\n",
                    marker, dirty, def.name, bar, pct, st.values[i]
                ));
            }

            // Hotkey legend — grouped, all on one line where they fit.
            s.push_str("\n  ↑/↓  select param");
            s.push_str("\n  ←/→  nudge       (Shift  ×10 step)");
            s.push_str("\n  Enter save        Esc / E  cancel");
            s.push_str("\n  *    = unsaved change");
        }

        // Skip the glyphon reshape if nothing changed since last frame.
        // The E overlay is re-drawn every frame while open; caching the
        // string eliminates the vast majority of set_text + shape calls
        // since the text only changes on keystroke.
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
