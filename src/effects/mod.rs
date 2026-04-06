pub mod manager;

use std::collections::HashMap;
use crate::gpu::{pipeline, EffectUniforms, GlobalUniforms};

/// All effects available in v1.
pub const EFFECT_NAMES: &[&str] = &[
    "hyperspace",
    "kaleido",
    "ring_tunnel",
    "warp_grid",
    "morph_geo",
    "spectrum_bars",
    "spectrum_orbit",
    "spectrum_terrain",
    "spectrum_wave",
    // v2 additions (techno / minimal / op-art set)
    "line_moire",
    "mandelbrot_zoom",
    "strange_attractor",
    "wire_tunnel",
    "voronoi_pulse",
];

// Shader sources embedded at compile time.
const GLOBALS_SRC: &str = include_str!("../../shaders/globals.wgsl");
const VERT_SRC: &str = include_str!("../../shaders/fullscreen.wgsl");

macro_rules! effect_src {
    ($name:literal) => {
        include_str!(concat!("../../shaders/effects/", $name, ".wgsl"))
    };
}

fn effect_source(name: &str) -> &'static str {
    match name {
        "hyperspace" => effect_src!("hyperspace"),
        "kaleido" => effect_src!("kaleido"),
        "ring_tunnel" => effect_src!("ring_tunnel"),
        "warp_grid" => effect_src!("warp_grid"),
        "morph_geo" => effect_src!("morph_geo"),
        "spectrum_bars" => effect_src!("spectrum_bars"),
        "spectrum_orbit" => effect_src!("spectrum_orbit"),
        "spectrum_terrain" => effect_src!("spectrum_terrain"),
        "spectrum_wave" => effect_src!("spectrum_wave"),
        // v2 additions
        "line_moire" => effect_src!("line_moire"),
        "mandelbrot_zoom" => effect_src!("mandelbrot_zoom"),
        "strange_attractor" => effect_src!("strange_attractor"),
        "wire_tunnel" => effect_src!("wire_tunnel"),
        "voronoi_pulse" => effect_src!("voronoi_pulse"),
        _ => panic!("Unknown effect: {}", name),
    }
}

#[allow(dead_code)]
pub struct EffectRegistry {
    pub global_bind_group_layout: wgpu::BindGroupLayout,
    pub effect_bind_group_layout: wgpu::BindGroupLayout,
    pub global_uniform_buffer: wgpu::Buffer,

    vert_shader: wgpu::ShaderModule,
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    effect_buffers: HashMap<String, wgpu::Buffer>,
    effect_bind_groups: HashMap<String, wgpu::BindGroup>,
}

impl EffectRegistry {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        enabled: Option<&[String]>,
    ) -> Self {
        // Global bind group layout: one uniform buffer
        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("global_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Effect bind group layout: one uniform buffer for EffectUniforms
        let effect_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("effect_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let global_uniform_buffer = pipeline::create_uniform_buffer(
            device,
            "global_uniforms",
            &GlobalUniforms::zeroed(),
        );

        let vert_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fullscreen_vert"),
            source: wgpu::ShaderSource::Wgsl(VERT_SRC.into()),
        });

        let names: Vec<&str> = match enabled {
            Some(list) => EFFECT_NAMES
                .iter()
                .filter(|&&n| list.iter().any(|e| e == n))
                .cloned()
                .collect(),
            None => EFFECT_NAMES.to_vec(),
        };

        let mut pipelines = HashMap::new();
        let mut effect_buffers = HashMap::new();
        let mut effect_bind_groups = HashMap::new();

        for name in &names {
            let frag_src = format!("{}\n{}", GLOBALS_SRC, effect_source(name));
            let frag_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{}_frag", name)),
                source: wgpu::ShaderSource::Wgsl(frag_src.into()),
            });

            let pipeline = pipeline::fullscreen_pipeline(
                device,
                name,
                &vert_shader,
                &frag_shader,
                &[&global_bind_group_layout, &effect_bind_group_layout],
                target_format,
            );

            let effect_uniforms = EffectUniforms::zeroed();
            let buf = pipeline::create_uniform_buffer(
                device,
                &format!("{}_params", name),
                &effect_uniforms,
            );

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("{}_bg", name)),
                layout: &effect_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });

            pipelines.insert(name.to_string(), pipeline);
            effect_buffers.insert(name.to_string(), buf);
            effect_bind_groups.insert(name.to_string(), bind_group);
        }

        Self {
            global_bind_group_layout,
            effect_bind_group_layout,
            global_uniform_buffer,
            vert_shader,
            pipelines,
            effect_buffers,
            effect_bind_groups,
        }
    }

    pub fn global_bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global_bg"),
            layout: &self.global_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.global_uniform_buffer.as_entire_binding(),
            }],
        })
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, uniforms: &GlobalUniforms) {
        queue.write_buffer(
            &self.global_uniform_buffer,
            0,
            bytemuck::bytes_of(uniforms),
        );
    }

    pub fn update_effect_params(
        &self,
        queue: &wgpu::Queue,
        name: &str,
        params: &EffectUniforms,
    ) {
        if let Some(buf) = self.effect_buffers.get(name) {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(params));
        }
    }

    pub fn render_effect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        name: &str,
        global_bg: &wgpu::BindGroup,
    ) {
        let pipeline = match self.pipelines.get(name) {
            Some(p) => p,
            None => {
                log::warn!("Effect '{}' not found", name);
                return;
            }
        };
        let effect_bg = &self.effect_bind_groups[name];

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("effect_pass"),
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
        pass.set_bind_group(0, global_bg, &[]);
        pass.set_bind_group(1, effect_bg, &[]);
        pass.draw(0..3, 0..1); // fullscreen triangle
    }

    pub fn effect_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.pipelines.keys().map(|s| s.as_str()).collect();
        // Preserve original order
        names.sort_by_key(|n| EFFECT_NAMES.iter().position(|&e| e == *n).unwrap_or(999));
        names
    }
}

/// Zeroed EffectUniforms (needed for buffer init)
trait Zeroed: Sized {
    fn zeroed() -> Self;
}
impl Zeroed for GlobalUniforms {
    fn zeroed() -> Self { bytemuck::Zeroable::zeroed() }
}
impl Zeroed for EffectUniforms {
    fn zeroed() -> Self { bytemuck::Zeroable::zeroed() }
}
