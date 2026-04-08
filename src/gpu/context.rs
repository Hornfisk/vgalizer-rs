use std::sync::Arc;
use winit::window::Window;

pub struct GpuContext {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub size: (u32, u32),
}

impl GpuContext {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = {
            let inner = window.inner_size();
            (inner.width.max(1), inner.height.max(1))
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });

        let surface = instance.create_surface(window).expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                // Prefer integrated GPU (saves power + works on old hardware)
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No compatible GPU adapter found");

        log::info!("GPU: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("vgalizer"),
                    // Use downlevel limits for max compatibility with old hardware
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    required_features: wgpu::Features::empty(),
                    memory_hints: Default::default(),
                },
                None, // trace_path
            )
            .await
            .expect("Failed to create GPU device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Fifo, // vsync
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Self { surface, device, queue, surface_config, size }
    }

    pub fn resize(&mut self, new_size: (u32, u32)) {
        if new_size.0 == 0 || new_size.1 == 0 {
            return;
        }
        self.size = new_size;
        self.surface_config.width = new_size.0;
        self.surface_config.height = new_size.1;
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    /// Create an offscreen RGBA texture for use as render target or sampler.
    pub fn create_offscreen_texture(&self, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: self.size.0,
                height: self.size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn create_linear_texture(&self, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
        self.create_linear_texture_sized(label, self.size.0, self.size.1)
    }

    /// Create an Rgba16Float offscreen texture at an explicit size.
    /// Used to back the internal-resolution effect + post-chain render
    /// targets when `render_scale < 1.0`.
    pub fn create_linear_texture_sized(
        &self,
        label: &str,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}

/// Derive the internal render size from a swapchain size and a scale
/// factor. Clamps scale to [0.5, 1.0] to prevent extreme downscales
/// that produce mush, and enforces a minimum 16-px dimension so the
/// GPU doesn't see zero-sized textures on tiny windows. At scale 1.0
/// this returns `swap` unchanged.
pub fn internal_size(scale: f32, swap: (u32, u32)) -> (u32, u32) {
    let s = scale.clamp(0.5, 1.0);
    (
        ((swap.0 as f32 * s).round() as u32).max(16),
        ((swap.1 as f32 * s).round() as u32).max(16),
    )
}
