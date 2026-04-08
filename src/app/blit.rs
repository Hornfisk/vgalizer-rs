//! Blit pipeline + CAS-lite sharpen upscale.
//!
//! The final render stage copies the post-processing chain's Rgba16Float
//! output texture (rendered at `internal_size`, possibly smaller than the
//! window) to the sRGB swapchain. When `render_scale < 1.0` this is also
//! the upscaling step; the fragment shader runs a cheap AMD-CAS-style
//! contrast-adaptive sharpen on top of the bilinear upsample so we
//! recover crispness that a plain linear blit would lose.
//!
//! This module owns:
//!   * `BlitUniforms` — the uniform buffer payload (inv_src_size, sharpen).
//!   * `BLIT_FRAG_SRC` — the WGSL shader for the CAS-lite blit.
//!   * `make_blit_pipeline` / `make_blit_bind_group` — pipeline + bind
//!     group builders.
//!   * `blit_*_entry` — bind-group layout entry helpers.
//!   * `rebuild_render_targets` — the one-shot rebuild for effect_tex,
//!     post_chain, blit bind group, and blit uniform buffer. Called on
//!     window resize and on `render_scale` hot-reload.

use super::AppState;
use crate::postprocess::PostProcessChain;

/// Blit uniform buffer layout. Must match `struct BlitUniforms` in
/// `BLIT_FRAG_SRC`. Fields are 16-byte aligned for std140 uniform rules.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct BlitUniforms {
    /// (1/src_width, 1/src_height). Used by the CAS-lite shader to
    /// sample cardinal neighbors one *source* pixel away, which is
    /// correct for sharpening a bilinearly upsampled image.
    inv_src_size: [f32; 2],
    /// Sharpen amount in [0, 1]. `0.0` = pure linear blit (no extra
    /// texture taps — the shader early-exits), higher = stronger CAS
    /// sharpening.
    sharpen: f32,
    _pad: f32,
}

impl BlitUniforms {
    pub(super) fn from_sizes(internal: (u32, u32), sharpen: f32) -> Self {
        Self {
            inv_src_size: [
                1.0 / internal.0.max(1) as f32,
                1.0 / internal.1.max(1) as f32,
            ],
            sharpen: sharpen.clamp(0.0, 1.0),
            _pad: 0.0,
        }
    }
}

/// CAS-lite upscale blit fragment shader. Reads an Rgba16Float internal
/// render texture and writes the sRGB swapchain with an optional
/// contrast-adaptive sharpen filter. When `sharpen == 0` it degenerates
/// to a single linear-sampled tap (cheapest possible upscale); when
/// `sharpen > 0` it adds a 5-tap CAS-style sharpen that recovers
/// crispness lost to the bilinear upsample.
///
/// Derived from AMD's public CAS reference; simplified: single-pass
/// (no separate EASU), luma-adaptive weight, sample taps at one source
/// pixel away so the sharpen works regardless of the dest/source ratio.
const BLIT_FRAG_SRC: &str = r#"
struct BlitUniforms {
    inv_src_size: vec2<f32>,
    sharpen: f32,
    _pad: f32,
};

@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_samp: sampler;
@group(0) @binding(2) var<uniform> u: BlitUniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32(i32(vertex_index) / 2) * 4.0 - 1.0;
    let y = f32(i32(vertex_index) % 2) * 4.0 - 1.0;
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let c = textureSample(input_tex, tex_samp, uv);
    if (u.sharpen <= 0.001) {
        return c;
    }
    let o = u.inv_src_size;
    let n = textureSample(input_tex, tex_samp, uv + vec2<f32>(0.0, -o.y)).rgb;
    let s = textureSample(input_tex, tex_samp, uv + vec2<f32>(0.0,  o.y)).rgb;
    let e = textureSample(input_tex, tex_samp, uv + vec2<f32>( o.x, 0.0)).rgb;
    let w = textureSample(input_tex, tex_samp, uv + vec2<f32>(-o.x, 0.0)).rgb;

    let mn = min(min(min(min(c.rgb, n), s), e), w);
    let mx = max(max(max(max(c.rgb, n), s), e), w);
    let mn_l = luma(mn);
    let mx_l = luma(mx);

    // CAS contrast adaptivity: less sharpening in already-saturated
    // regions (near 0 or 1 luma) so we don't amplify noise.
    let amp_in = min(mn_l, 1.0 - mx_l) / max(mx_l, 1e-4);
    let amp = sqrt(clamp(amp_in, 0.0, 1.0));

    // Map user sharpen 0..1 to CAS peak -0.125..-0.2 (AMD's useful range).
    let peak = -0.125 - 0.075 * u.sharpen;
    let weight = amp * peak;

    let sum = (n + s + e + w) * weight;
    let denom = 1.0 + 4.0 * weight;
    return vec4<f32>((c.rgb + sum) / denom, c.a);
}
"#;

/// Build the blit pass bind group. Called once at startup and again
/// whenever `post_chain.final_view()` identity changes (window resize or
/// render_scale hot-reload).
pub(super) fn make_blit_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    src_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit_bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform_buf.as_entire_binding(),
            },
        ],
    })
}

/// Rebuild the effect + post-chain render targets and the blit bind
/// group using the current `config.render_scale` against the current
/// swapchain size. Called on window resize and on `render_scale`
/// hot-reload. Also rewrites the blit uniform buffer so inv_src_size
/// matches the new internal render resolution.
pub(super) fn rebuild_render_targets(s: &mut AppState) {
    let new_internal = crate::gpu::internal_size(s.config.render_scale, s.gpu.size);
    log::info!(
        "render targets: swap {}x{} internal {}x{} (scale {:.2})",
        s.gpu.size.0, s.gpu.size.1,
        new_internal.0, new_internal.1,
        s.config.render_scale
    );
    s.internal_size = new_internal;

    let (effect_tex, effect_view) = s.gpu.create_linear_texture_sized(
        "effect_output", new_internal.0, new_internal.1);
    s.effect_tex = effect_tex;
    s.effect_view = effect_view;
    s.post_chain = PostProcessChain::new(
        &s.gpu,
        &s.effects.global_uniform_buffer,
        &s.effect_view,
        new_internal,
    );
    s.gpu.queue.write_buffer(
        &s.blit_uniform_buf,
        0,
        bytemuck::bytes_of(&BlitUniforms::from_sizes(new_internal, s.config.upscale_sharpen)),
    );
    s.blit_bg = make_blit_bind_group(
        &s.gpu.device,
        &s.blit_bgl,
        s.post_chain.final_view(),
        &s.blit_sampler,
        &s.blit_uniform_buf,
    );
}

pub(super) fn make_blit_pipeline(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    target_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blit_shader"),
        source: wgpu::ShaderSource::Wgsl(BLIT_FRAG_SRC.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blit_layout"),
        bind_group_layouts: &[bgl],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blit_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

pub(super) fn blit_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

pub(super) fn blit_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub(super) fn blit_uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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
