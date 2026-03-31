use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use crate::audio::{AtomicAudioState, BeatTracker};
use crate::colors::palette;
use crate::config::Config;
use crate::effects::{manager::SceneManager, EffectRegistry};
use crate::gpu::{EffectUniforms, GlobalUniforms, PostUniforms};
use crate::gpu::uniforms::pack_bands;
use crate::input::{Action, InputHandler};
use crate::overlay::HudOverlay;
use crate::postprocess::PostProcessChain;
use crate::text::NameOverlay;

pub fn run(config: Config) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App { config, state: None };
    event_loop.run_app(&mut app).expect("Event loop failed");
}

struct App {
    config: Config,
    state: Option<AppState>,
}

struct AppState {
    window: Arc<Window>,
    gpu: crate::gpu::GpuContext,
    effects: EffectRegistry,
    global_bg: wgpu::BindGroup,
    post_chain: PostProcessChain,
    effect_tex: wgpu::Texture,
    effect_view: wgpu::TextureView,
    // Blit pipeline: copies post result (Rgba16Float) to swapchain (sRGB)
    blit_pipeline: wgpu::RenderPipeline,
    blit_bgl: wgpu::BindGroupLayout,

    name_overlay: NameOverlay,
    hud: HudOverlay,
    input: InputHandler,
    scene: SceneManager,
    beat_tracker: BeatTracker,
    audio_state: Arc<AtomicAudioState>,
    _audio_stream: cpal::Stream,
    config_watcher: Option<crate::config::ConfigWatcher>,
    config: Config,

    // Timing
    start: Instant,
    last_frame: Instant,
    last_beat_t: f64,
    pulse: f32,
    rotation_angle: f32,
    strobe_alpha: f32,
    sensitivity: f32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let config = &self.config;

        // Build window
        let (w, h) = config.resolution.unwrap_or((1920, 1080));
        let attrs = Window::default_attributes()
            .with_title("vgalizer")
            .with_inner_size(LogicalSize::new(w, h));
        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));

        if config.fullscreen {
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }

        // Init GPU
        let gpu = pollster::block_on(crate::gpu::GpuContext::new(window.clone()));

        // Effect registry
        let effects = EffectRegistry::new(
            &gpu.device,
            &gpu.queue,
            // Effects render to Rgba16Float; only blit to swapchain uses sRGB
            wgpu::TextureFormat::Rgba16Float,
            config.enabled_effects.as_deref(),
        );
        let global_bg = effects.global_bind_group(&gpu.device);

        // Effect render target
        let (effect_tex, effect_view) = gpu.create_linear_texture("effect_output");

        // Post-processing
        let post_chain = PostProcessChain::new(&gpu, &effects.global_uniform_buffer);

        // Blit pipeline: copies Rgba16Float → swapchain sRGB
        let blit_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit_bgl"),
            entries: &[
                blit_tex_entry(0),
                blit_sampler_entry(1),
            ],
        });
        let blit_pipeline = make_blit_pipeline(&gpu.device, &blit_bgl, gpu.surface_format());

        let screen_size = gpu.size;

        // Scene manager
        let effect_names = effects.effect_names().iter().map(|s| s.to_string()).collect();
        let scene = SceneManager::new(effect_names, &config.mirror_pool, config.scene_duration);

        // Audio
        let audio_state = Arc::new(AtomicAudioState::new());
        let stream = match crate::audio::capture::start_capture(
            config.audio_device.as_deref(),
            audio_state.clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Audio error: {}. Running silently.", e);
                // Create a silent fallback — we can't easily create an empty stream,
                // so we just proceed; audio_state will stay at zero
                return;
            }
        };

        let config_watcher = crate::config::ConfigWatcher::new(&self.config_path());

        let name_overlay = NameOverlay::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            screen_size,
            &config.dj_name,
            config.name_font_size,
        );

        let hud = HudOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());

        let beat_tracker = BeatTracker::new(config.beat_sensitivity);

        self.state = Some(AppState {
            window,
            gpu,
            effects,
            global_bg,
            post_chain,
            effect_tex,
            effect_view,
            blit_pipeline,
            blit_bgl,
            name_overlay,
            hud,
            input: InputHandler::new(),
            scene,
            beat_tracker,
            audio_state,
            _audio_stream: stream,
            config_watcher,
            config: self.config.clone(),
            start: Instant::now(),
            last_frame: Instant::now(),
            last_beat_t: 0.0,
            pulse: 0.0,
            rotation_angle: 0.0,
            strobe_alpha: 0.0,
            sensitivity: self.config.beat_sensitivity,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if let Some(action) = state.input.handle(&key_event) {
                    match action {
                        Action::Quit => event_loop.exit(),
                        Action::NextEffect => state.scene.advance(),
                        Action::JumpTo(i) => state.scene.jump_to(i),
                        Action::SensitivityUp => {
                            state.sensitivity = (state.sensitivity + 0.1).min(3.0);
                            state.beat_tracker.set_sensitivity(state.sensitivity);
                        }
                        Action::SensitivityDown => {
                            state.sensitivity = (state.sensitivity - 0.1).max(0.5);
                            state.beat_tracker.set_sensitivity(state.sensitivity);
                        }
                        Action::CyclePostMode => state.scene.cycle_mirror(),
                        Action::ToggleHelp => state.hud.toggle(),
                        Action::ToggleFullscreen => {
                            state.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                        Action::ToggleWindowed => {
                            state.window.set_fullscreen(None);
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                let s = &mut self.state.as_mut().unwrap();
                s.gpu.resize((size.width.max(1), size.height.max(1)));
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

impl App {
    fn config_path(&self) -> String {
        "config.json".to_string()
    }

    fn render_frame(&mut self) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Hot-reload config
        if let Some(watcher) = &mut state.config_watcher {
            if let Some(new_cfg) = watcher.poll() {
                if new_cfg.dj_name != state.config.dj_name {
                    state.name_overlay.set_name(&new_cfg.dj_name);
                }
                if (new_cfg.beat_sensitivity - state.config.beat_sensitivity).abs() > 0.001 {
                    state.sensitivity = new_cfg.beat_sensitivity;
                    state.beat_tracker.set_sensitivity(state.sensitivity);
                }
                state.config = new_cfg;
            }
        }

        let now = Instant::now();
        let t = state.start.elapsed().as_secs_f64();
        let dt = state.last_frame.elapsed().as_secs_f64().min(0.05) as f32;
        state.last_frame = now;

        // Read audio
        let level = state.audio_state.load_level();
        let bands = state.audio_state.load_bands();

        // Beat detection
        let beat_state = state.beat_tracker.update(level, t);

        // Update pulse (decays between beats)
        if beat_state.beat {
            state.pulse = 1.0;
            state.last_beat_t = t;
        }
        state.pulse *= 0.92; // Decay

        // Strobe
        let strobe_on = match state.config.strobe_mode.as_str() {
            "beat" => beat_state.beat,
            "half" => beat_state.half_beat,
            "quarter" => beat_state.quarter_beat,
            _ => beat_state.beat,
        };
        if strobe_on {
            state.strobe_alpha = 0.7;
        } else {
            state.strobe_alpha = (state.strobe_alpha - 3.0 * dt).max(0.0);
        }

        // Rotation spring (slowly oscillates, beat kick)
        let rot_target = if beat_state.beat { state.config.global_rotation * 0.02 } else { 0.0 };
        state.rotation_angle = state.rotation_angle * 0.95 + rot_target * 0.05;

        // Update scene
        state.scene.update(&beat_state);

        // Current scene state
        let effect_name = state.scene.current_effect().to_string();
        let pal_idx = state.scene.current_palette_index();
        let pal = palette(pal_idx);
        let mirror = state.scene.current_mirror();
        let beat_time = (t - state.last_beat_t) as f32;

        // Build GlobalUniforms
        let globals = GlobalUniforms {
            time: t as f32,
            dt,
            beat_time,
            fx_speed: state.config.fx_speed_mult,
            resolution: [state.gpu.size.0 as f32, state.gpu.size.1 as f32],
            _pad1: [0.0; 2],
            level,
            pulse: state.pulse,
            beat: if beat_state.beat { 1.0 } else { 0.0 },
            half_beat: if beat_state.half_beat { 1.0 } else { 0.0 },
            quarter_beat: if beat_state.quarter_beat { 1.0 } else { 0.0 },
            bpm: beat_state.bpm,
            _pad2: [0.0; 2],
            bands: pack_bands(&bands),
            palette_sa: pal.sa4(),
            palette_sb: pal.sb4(),
            palette_ra: pal.ra4(),
            palette_rb: pal.rb4(),
        };

        // Build PostUniforms
        let strobe_col = pal.sa;
        let post = PostUniforms {
            trail_alpha: state.config.trail_alpha as f32,
            glitch_intensity: state.config.glitch_intensity * level,
            vga_intensity: state.config.vga_intensity,
            vga_ca: state.config.vga_ca as f32,
            vga_noise: state.config.vga_noise,
            vga_sync: state.config.vga_sync,
            rotation_angle: state.rotation_angle,
            vibration_y: 0.0,
            strobe_alpha: state.strobe_alpha,
            strobe_r: strobe_col[0],
            strobe_g: strobe_col[1],
            strobe_b: strobe_col[2],
            mirror_mode: mirror.as_u32(),
            mirror_alpha: state.config.mirror_alpha as f32,
            mirror_count: state.config.mirror_count,
            mirror_spread: state.config.mirror_spread as f32,
        };

        // Upload uniforms
        state.effects.update_globals(&state.gpu.queue, &globals);
        state.post_chain.update_globals(&state.gpu.queue, &globals);
        state.post_chain.update_post(&state.gpu.queue, &post);

        // Update name overlay animation
        state.name_overlay.update(beat_state.beat, state.pulse);

        // HUD text
        state.hud.update_text(&effect_name, beat_state.bpm, state.sensitivity);

        // Acquire swapchain frame
        let output = match state.gpu.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost) => {
                let size = state.gpu.size;
                state.gpu.resize(size);
                return;
            }
            Err(wgpu::SurfaceError::Outdated) => return,
            Err(e) => {
                log::error!("Surface error: {:?}", e);
                return;
            }
        };
        let output_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("frame") }
        );

        // --- Effect pass ---
        state.effects.render_effect(
            &mut encoder,
            &state.effect_view,
            &effect_name,
            &state.global_bg,
        );

        // --- Post-processing chain ---
        let final_view = state.post_chain.process(
            &mut encoder,
            &state.gpu.device,
            &state.effect_view,
            &post,
        );

        // --- Blit to swapchain ---
        {
            let sampler = crate::gpu::pipeline::create_sampler(&state.gpu.device);
            let blit_bg = state.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit_bg"),
                layout: &state.blit_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(final_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
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
            pass.set_pipeline(&state.blit_pipeline);
            pass.set_bind_group(0, &blit_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // --- Name overlay (rendered directly to swapchain) ---
        state.name_overlay.render(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &output_view,
            state.gpu.size,
            &pal,
            state.pulse,
            beat_state.beat,
        );

        // --- HUD overlay ---
        state.hud.render(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &output_view,
            state.gpu.size,
        );

        state.gpu.queue.submit([encoder.finish()]);
        output.present();
    }
}

// --- Blit pipeline helpers ---

const BLIT_FRAG_SRC: &str = r#"
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_samp: sampler;

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

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSample(input_tex, tex_samp, uv);
}
"#;

fn make_blit_pipeline(
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

fn blit_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

fn blit_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
