pub mod manager;
pub mod params;

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
    "wire_tunnel",
    "voronoi_pulse",
    // v3 additions (vector / scope / cymatics set)
    "vector_terrain",
    "laser_burst",
    "scope_xy",
    "wave_dunes",
    "radial_eq",
    "harmonograph",
    "tv_acid",
    "kaleido_warp",
    "isoline_field",
    "moebius_grid",
    "cymatics",
    "vector_rabbit",
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
        "wire_tunnel" => effect_src!("wire_tunnel"),
        "voronoi_pulse" => effect_src!("voronoi_pulse"),
        // v3 additions
        "vector_terrain" => effect_src!("vector_terrain"),
        "laser_burst" => effect_src!("laser_burst"),
        "scope_xy" => effect_src!("scope_xy"),
        "wave_dunes" => effect_src!("wave_dunes"),
        "radial_eq" => effect_src!("radial_eq"),
        "harmonograph" => effect_src!("harmonograph"),
        "tv_acid" => effect_src!("tv_acid"),
        "kaleido_warp" => effect_src!("kaleido_warp"),
        "isoline_field" => effect_src!("isoline_field"),
        "moebius_grid" => effect_src!("moebius_grid"),
        "cymatics" => effect_src!("cymatics"),
        "vector_rabbit" => effect_src!("vector_rabbit"),
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
    /// CPU-side mirror of the params currently uploaded for each effect.
    /// Lets the param editor seed itself with the live values.
    effect_params_cache: HashMap<String, EffectUniforms>,
}

impl EffectRegistry {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
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

        // Always build pipelines for every effect — runtime enable/disable
        // is handled by SceneManager so it can toggle without rebuilding.
        let names: Vec<&str> = EFFECT_NAMES.to_vec();

        let mut pipelines = HashMap::new();
        let mut effect_buffers = HashMap::new();
        let mut effect_bind_groups = HashMap::new();
        let mut effect_params_cache = HashMap::new();

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
            effect_params_cache.insert(name.to_string(), effect_uniforms);
        }

        Self {
            global_bind_group_layout,
            effect_bind_group_layout,
            global_uniform_buffer,
            vert_shader,
            pipelines,
            effect_buffers,
            effect_bind_groups,
            effect_params_cache,
        }
    }

    /// Read the CPU-side copy of an effect's current params (for the editor).
    pub fn current_params(&self, name: &str) -> Option<&EffectUniforms> {
        self.effect_params_cache.get(name)
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
        &mut self,
        queue: &wgpu::Queue,
        name: &str,
        params: &EffectUniforms,
    ) {
        if let Some(buf) = self.effect_buffers.get(name) {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(params));
        }
        self.effect_params_cache.insert(name.to_string(), *params);
    }

    /// Force driver-side shader compilation for every registered effect
    /// by submitting one throwaway draw per pipeline into `target`.
    ///
    /// wgpu creates pipelines eagerly in `EffectRegistry::new`, but Mesa's
    /// i965/iris backend defers the WGSL→SPIR-V→Gen9 native lowering to
    /// the first real draw call. That shows up as a per-effect stall on
    /// the first scene-switch after startup — most visible right after
    /// boot when the kernel page cache hasn't loaded `mesa_shader_cache_db`.
    ///
    /// Running a one-triangle draw per pipeline here pays that cost once
    /// upfront before the winit event loop starts. On UHD 620 Gen9 with
    /// 25 effects this should add ~1–2 s to startup. We block on
    /// `device.poll(Wait)` at the end so the caller can assume every
    /// pipeline is compiled when `prewarm` returns. See T2 in the debug
    /// plan.
    pub fn prewarm(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        global_bg: &wgpu::BindGroup,
    ) {
        let t0 = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("effect_prewarm"),
        });
        for name in self.effect_names() {
            let pipeline = &self.pipelines[name];
            let effect_bg = &self.effect_bind_groups[name];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("prewarm_pass"),
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
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        // Block until the driver has finished compiling + executing every
        // prewarm draw. Without this the calls are just queued and the
        // first real frame still pays the compile cost.
        let _ = device.poll(wgpu::Maintain::Wait);
        log::info!(
            "prewarm: compiled {} effect pipelines in {:.1} ms",
            self.pipelines.len(),
            t0.elapsed().as_secs_f64() * 1000.0,
        );
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
