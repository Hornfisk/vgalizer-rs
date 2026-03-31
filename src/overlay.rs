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
        buffer.set_size(&mut font_system, Some(400.0), Some(200.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            visible: true,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn update_text(&mut self, effect: &str, bpm: f32, sensitivity: f32) {
        let text = format!(
            "Effect: {}\nBPM: {:.0}  Sens: {:.1}\nSPACE next  1-9 jump  +/- sens  P mirror  H hide  Q quit",
            effect, bpm, sensitivity
        );
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
        if !self.visible { return; }

        self.viewport.update(queue, Resolution {
            width: screen_size.0,
            height: screen_size.1,
        });

        let areas = [TextArea {
            buffer: &self.buffer,
            left: 12.0,
            top: 12.0,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: 600,
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
