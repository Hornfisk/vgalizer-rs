use wgpu::util::DeviceExt;

use crate::gpu::{pipeline, GpuContext, PostUniforms};

const VERT_SRC: &str = include_str!("../../shaders/fullscreen.wgsl");
const TRAIL_SRC: &str = include_str!("../../shaders/post/trail.wgsl");
const MIRROR_SRC: &str = include_str!("../../shaders/post/mirror.wgsl");
const ROTATION_SRC: &str = include_str!("../../shaders/post/rotation.wgsl");
const GLITCH_SRC: &str = include_str!("../../shaders/post/glitch.wgsl");
const VGA_SRC: &str = include_str!("../../shaders/post/vga.wgsl");
const SCANLINES_SRC: &str = include_str!("../../shaders/post/scanlines.wgsl");
const STROBE_SRC: &str = include_str!("../../shaders/post/strobe.wgsl");

/// Manages the full post-processing chain using ping-pong textures.
pub struct PostProcessChain {
    // Ping-pong textures
    tex_a: wgpu::Texture,
    view_a: wgpu::TextureView,
    tex_b: wgpu::Texture,
    view_b: wgpu::TextureView,

    // Persistent trail buffer
    trail_tex: wgpu::Texture,
    trail_view: wgpu::TextureView,

    // Shared sampler
    sampler: wgpu::Sampler,

    // Post uniform buffer (written each frame)
    post_buf: wgpu::Buffer,

    // Global uniform buffer ref (for shaders that need audio data)
    global_buf_ref: wgpu::Buffer,

    // Pipelines
    trail_pipeline: wgpu::RenderPipeline,
    mirror_pipeline: wgpu::RenderPipeline,
    rotation_pipeline: wgpu::RenderPipeline,
    glitch_pipeline: wgpu::RenderPipeline,
    vga_pipeline: wgpu::RenderPipeline,
    scanlines_pipeline: wgpu::RenderPipeline,
    strobe_pipeline: wgpu::RenderPipeline,

    // Bind group layouts
    trail_bgl: wgpu::BindGroupLayout,
    mirror_bgl: wgpu::BindGroupLayout,
    rotation_bgl: wgpu::BindGroupLayout,
    glitch_bgl: wgpu::BindGroupLayout,
    vga_bgl: wgpu::BindGroupLayout,
    scanlines_bgl: wgpu::BindGroupLayout,
    strobe_bgl: wgpu::BindGroupLayout,
}

impl PostProcessChain {
    pub fn new(gpu: &GpuContext, global_buf: &wgpu::Buffer) -> Self {
        let device = &gpu.device;
        let format = wgpu::TextureFormat::Rgba16Float;

        let (tex_a, view_a) = gpu.create_linear_texture("post_tex_a");
        let (tex_b, view_b) = gpu.create_linear_texture("post_tex_b");
        let (trail_tex, trail_view) = gpu.create_linear_texture("trail_buf");

        let sampler = pipeline::create_sampler(device);
        let post_buf = pipeline::create_uniform_buffer(device, "post_uniforms", &PostUniforms::zeroed());

        let vert = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post_vert"),
            source: wgpu::ShaderSource::Wgsl(VERT_SRC.into()),
        });

        // --- Trail ---
        let trail_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("trail_bgl"),
            entries: &[
                tex_binding(0), tex_binding(1),
                sampler_binding(2), uniform_binding(3),
            ],
        });
        let trail_pipeline = make_pipeline(device, "trail", &vert,
            TRAIL_SRC, &[&trail_bgl], format);

        // --- Mirror ---
        let mirror_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror_bgl"),
            entries: &[tex_binding(0), sampler_binding(1), uniform_binding(2)],
        });
        let mirror_pipeline = make_pipeline(device, "mirror", &vert,
            MIRROR_SRC, &[&mirror_bgl], format);

        // --- Rotation + vibration ---
        let rotation_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rotation_bgl"),
            entries: &[tex_binding(0), sampler_binding(1), uniform_binding(2)],
        });
        let rotation_pipeline = make_pipeline(device, "rotation", &vert,
            ROTATION_SRC, &[&rotation_bgl], format);

        // --- Glitch (needs globals for time) ---
        let glitch_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glitch_bgl"),
            entries: &[uniform_binding(0), tex_binding(1), sampler_binding(2), uniform_binding(3)],
        });
        let glitch_pipeline = make_pipeline(device, "glitch", &vert,
            GLITCH_SRC, &[&glitch_bgl], format);

        // --- VGA (needs globals for time/resolution) ---
        let vga_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vga_bgl"),
            entries: &[uniform_binding(0), tex_binding(1), sampler_binding(2), uniform_binding(3)],
        });
        let vga_pipeline = make_pipeline(device, "vga", &vert,
            VGA_SRC, &[&vga_bgl], format);

        // --- Scanlines (needs globals for resolution) ---
        let scanlines_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scanlines_bgl"),
            entries: &[uniform_binding(0), tex_binding(1), sampler_binding(2), uniform_binding(3)],
        });
        let scanlines_pipeline = make_pipeline(device, "scanlines", &vert,
            SCANLINES_SRC, &[&scanlines_bgl], format);

        // --- Strobe ---
        let strobe_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strobe_bgl"),
            entries: &[tex_binding(0), sampler_binding(1), uniform_binding(2)],
        });
        let strobe_pipeline = make_pipeline(device, "strobe", &vert,
            STROBE_SRC, &[&strobe_bgl], format);

        // Clone the global buffer reference by creating a small wrapper
        // Actually we need to create a separate reference; store the raw buffer
        // We'll hold onto global_buf by having caller pass it per frame
        let global_buf_ref = pipeline::create_uniform_buffer(
            device, "post_global_ref", &crate::gpu::GlobalUniforms::zeroed());

        Self {
            tex_a, view_a, tex_b, view_b,
            trail_tex, trail_view,
            sampler, post_buf, global_buf_ref,
            trail_pipeline, mirror_pipeline, rotation_pipeline, glitch_pipeline,
            vga_pipeline, scanlines_pipeline, strobe_pipeline,
            trail_bgl, mirror_bgl, rotation_bgl, glitch_bgl, vga_bgl, scanlines_bgl, strobe_bgl,
        }
    }

    /// Update the shared global uniforms copy used by post shaders.
    pub fn update_globals(&self, queue: &wgpu::Queue, uniforms: &crate::gpu::GlobalUniforms) {
        queue.write_buffer(&self.global_buf_ref, 0, bytemuck::bytes_of(uniforms));
    }

    /// Update post uniforms.
    pub fn update_post(&self, queue: &wgpu::Queue, uniforms: &PostUniforms) {
        queue.write_buffer(&self.post_buf, 0, bytemuck::bytes_of(uniforms));
    }

    /// Run the full post-processing chain.
    /// Input: effect_view (the rendered effect texture view).
    /// Returns: view of the final processed texture.
    pub fn process<'a>(
        &'a self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        effect_view: &wgpu::TextureView,
        post: &PostUniforms,
    ) -> &'a wgpu::TextureView {
        // Pass 1: Trail blend. Read trail_tex (prev frame) + effect → write tex_a.
        // tex_a becomes the new trail; we copy it to trail_tex after the frame.
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("trail_bg"),
                layout: &self.trail_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.trail_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(effect_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "trail_pass", &self.view_a, &self.trail_pipeline, &bg);
        }

        // Copy tex_a → trail_tex so next frame has the current trail as its "previous"
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &self.tex_a,
                mip_level: 0, origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: &self.trail_tex,
                mip_level: 0, origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: self.tex_a.size().width, height: self.tex_a.size().height, depth_or_array_layers: 1 },
        );

        // Pass 2: Mirror (tex_a → tex_b)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mirror_bg"),
                layout: &self.mirror_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view_a) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "mirror_pass", &self.view_b, &self.mirror_pipeline, &bg);
        }

        // Pass 3: Rotation + vibration (tex_b → tex_a)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rotation_bg"),
                layout: &self.rotation_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view_b) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "rotation_pass", &self.view_a, &self.rotation_pipeline, &bg);
        }

        // Pass 4: Strobe (tex_a → tex_b) — passthrough when strobe_alpha=0
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("strobe_bg"),
                layout: &self.strobe_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view_a) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "strobe_pass", &self.view_b, &self.strobe_pipeline, &bg);
        }

        // Pass 5: Glitch (tex_b → tex_a)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glitch_bg"),
                layout: &self.glitch_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.global_buf_ref.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view_b) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "glitch_pass", &self.view_a, &self.glitch_pipeline, &bg);
        }

        // Pass 6: VGA (tex_a → tex_b)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("vga_bg"),
                layout: &self.vga_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.global_buf_ref.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view_a) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "vga_pass", &self.view_b, &self.vga_pipeline, &bg);
        }

        // Pass 7: Scanlines (tex_b → tex_a)
        {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scanlines_bg"),
                layout: &self.scanlines_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.global_buf_ref.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view_b) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: self.post_buf.as_entire_binding() },
                ],
            });
            render_pass(encoder, "scanlines_pass", &self.view_a, &self.scanlines_pipeline, &bg);
        }

        &self.view_a  // final output (scanlines wrote to tex_a)
    }
}

fn tex_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    }
}

fn sampler_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn uniform_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    label: &str,
    vert: &wgpu::ShaderModule,
    frag_src: &str,
    bgls: &[&wgpu::BindGroupLayout],
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let frag = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("{}_frag", label)),
        source: wgpu::ShaderSource::Wgsl(frag_src.into()),
    });
    pipeline::fullscreen_pipeline(device, label, vert, &frag, bgls, format)
}

fn render_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

trait Zeroed: Sized {
    fn zeroed() -> Self;
}
impl Zeroed for PostUniforms {
    fn zeroed() -> Self { bytemuck::Zeroable::zeroed() }
}
impl Zeroed for crate::gpu::GlobalUniforms {
    fn zeroed() -> Self { bytemuck::Zeroable::zeroed() }
}
