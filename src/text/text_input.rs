/// Modal centered text-input overlay for editing the DJ name live.
/// Opened via the `T` key. Enter confirms, Escape cancels, Backspace pops.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

pub struct TextInputOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    font_size_px: f32,
    cursor_t: f32,
}

impl TextInputOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let font_data = include_bytes!("../../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size_px = 64.0;
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(font_size_px, font_size_px * 1.25),
        );
        buffer.set_size(&mut font_system, Some(4000.0), Some(200.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            font_size_px,
            cursor_t: 0.0,
        }
    }

    /// Advance blinking cursor animation.
    pub fn tick(&mut self, dt: f32) {
        self.cursor_t = (self.cursor_t + dt) % 1.0;
    }

    /// Update the text shown in the overlay. `buffer_text` is the current
    /// in-progress DJ name edit buffer.
    pub fn update_text(&mut self, buffer_text: &str) {
        let cursor = if self.cursor_t < 0.5 { "_" } else { " " };
        let shown = format!("DJ NAME:  {}{}\n[ENTER] save   [ESC] cancel   [BACKSPACE] erase", buffer_text, cursor);
        self.buffer.set_text(
            &mut self.font_system,
            &shown,
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

        // Measure layout to center it horizontally and vertically.
        let layout_width: f32 = self.buffer.layout_runs()
            .map(|run| run.line_w)
            .fold(0.0f32, f32::max);
        let layout_height = self.font_size_px * 1.25 * 2.0;

        let left = (screen_size.0 as f32 - layout_width) * 0.5;
        let top  = (screen_size.1 as f32 - layout_height) * 0.5;

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
            .expect("TextInput prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text_input_pass"),
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
                .expect("TextInput render failed");
        }

        self.atlas.trim();
    }
}
