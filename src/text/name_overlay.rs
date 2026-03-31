/// DJ name overlay with chromatic aberration and beat-synced pulse/jitter.
/// Uses glyphon for GPU text rendering.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use rand::Rng;

use crate::colors::Palette;

pub struct NameOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    name: String,
    font_size_px: f32,
    jitter_x: f32,
    jitter_y: f32,
    rng: rand::rngs::ThreadRng,
}

impl NameOverlay {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        screen_size: (u32, u32),
        name: &str,
        font_size_frac: f32,
    ) -> Self {
        let mut font_system = FontSystem::new();

        // Load bundled Roboto Condensed Bold
        let font_data = include_bytes!("../../assets/fonts/RobotoCondensed-Bold.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size_px = (screen_size.1 as f32 * font_size_frac).max(24.0);
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size_px, font_size_px));
        buffer.set_size(&mut font_system, Some(screen_size.0 as f32), Some(screen_size.1 as f32));
        buffer.set_text(&mut font_system, name, Attrs::new().family(Family::Name("Roboto Condensed")), Shaping::Basic);
        buffer.shape_until_scroll(&mut font_system, false);

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            name: name.to_string(),
            font_size_px,
            jitter_x: 0.0,
            jitter_y: 0.0,
            rng: rand::thread_rng(),
        }
    }

    pub fn set_name(&mut self, name: &str) {
        if name != self.name {
            self.name = name.to_string();
            self.buffer.set_text(
                &mut self.font_system,
                name,
                Attrs::new().family(Family::Name("Roboto Condensed")),
                Shaping::Basic,
            );
            self.buffer.shape_until_scroll(&mut self.font_system, false);
        }
    }

    pub fn update(&mut self, beat: bool, pulse: f32) {
        if beat {
            self.jitter_x = self.rng.gen_range(-12.0..12.0);
            self.jitter_y = self.rng.gen_range(-5.0..5.0);
        } else {
            self.jitter_x *= 0.8;
            self.jitter_y *= 0.8;
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        screen_size: (u32, u32),
        palette: &Palette,
        pulse: f32,
        beat: bool,
    ) {
        self.viewport.update(queue, Resolution {
            width: screen_size.0,
            height: screen_size.1,
        });

        // Measure text layout to center it
        let layout_width: f32 = self.buffer.layout_runs()
            .map(|run| run.line_w)
            .fold(0.0f32, f32::max);
        let layout_height = self.font_size_px;

        let cx = screen_size.0 as f32 / 2.0;
        let cy = screen_size.1 as f32 * 0.62; // lower-center like Python

        let base_scale = 1.0 + pulse * 0.07 + if beat { 0.06 } else { 0.0 };
        let scaled_w = layout_width * base_scale;

        let left = cx - scaled_w / 2.0 + self.jitter_x;
        let top  = cy - layout_height / 2.0 + self.jitter_y;

        let ca_px = 6.0; // chromatic aberration pixel offset

        // We render 3 passes for chromatic aberration:
        // Red shifted left, blue shifted right, white center
        let sa = palette.sa;
        let sb = palette.sb;

        let areas = [
            // Red-ish (palette_sa), shifted left
            TextArea {
                buffer: &self.buffer,
                left: left - ca_px,
                top,
                scale: base_scale,
                bounds: TextBounds {
                    left: 0, top: 0,
                    right: screen_size.0 as i32,
                    bottom: screen_size.1 as i32,
                },
                default_color: Color::rgba(
                    (sa[0] * 255.0) as u8,
                    (sa[1] * 255.0) as u8,
                    (sa[2] * 255.0) as u8,
                    200,
                ),
                custom_glyphs: &[],
            },
            // Blue-ish (palette_sb), shifted right
            TextArea {
                buffer: &self.buffer,
                left: left + ca_px,
                top,
                scale: base_scale,
                bounds: TextBounds {
                    left: 0, top: 0,
                    right: screen_size.0 as i32,
                    bottom: screen_size.1 as i32,
                },
                default_color: Color::rgba(
                    (sb[0] * 255.0) as u8,
                    (sb[1] * 255.0) as u8,
                    (sb[2] * 255.0) as u8,
                    200,
                ),
                custom_glyphs: &[],
            },
            // White center
            TextArea {
                buffer: &self.buffer,
                left,
                top,
                scale: base_scale,
                bounds: TextBounds {
                    left: 0, top: 0,
                    right: screen_size.0 as i32,
                    bottom: screen_size.1 as i32,
                },
                default_color: Color::rgba(255, 255, 255, 230),
                custom_glyphs: &[],
            },
        ];

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
            .expect("Text prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Draw on top of existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .expect("Text render failed");
        }

        self.atlas.trim();
    }
}
