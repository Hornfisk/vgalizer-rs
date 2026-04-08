use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

mod blit;
mod frame;

use blit::{
    blit_sampler_entry, blit_tex_entry, blit_uniform_entry,
    make_blit_bind_group, make_blit_pipeline, rebuild_render_targets, BlitUniforms,
};

use crate::audio::{AtomicAudioState, BeatTracker};
use crate::audio_picker::{AudioPicker, AudioPickerOverlay};
use crate::effects_menu::{EffectsMenuOverlay, EffectsMenuState};
use crate::global_settings::{GlobalKnob, GlobalSettingsOverlay, GlobalSettingsState};
use crate::config::Config;
use crate::effects::{manager::SceneManager, EffectRegistry};
use crate::input::{Action, InputHandler};
use crate::overlay::HudOverlay;
use crate::postprocess::PostProcessChain;
use crate::text::{
    NameOverlay, ParamEditState, ParamsOverlay, TextInputOverlay,
    VjeEffectsFocus, VjeOverlay, VjeOverlayState, VjeTab,
};

pub fn run(config: Config, config_path: String) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App { config, config_path, state: None };
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("Event loop exited with error: {:?}", e);
    }
}

struct App {
    config: Config,
    config_path: String,
    state: Option<AppState>,
}

/// All fields are `pub(super)` so `app` submodules (`blit.rs`,
/// `frame.rs`) can read and mutate them directly. Visibility is scoped
/// to the `app` module tree only — nothing leaks to the rest of the
/// crate. This is the "app-private shared state" pattern: the whole
/// struct is a local data bundle for code that all lives under `app/`.
#[allow(dead_code)]
pub(super) struct AppState {
    pub(super) window: Arc<Window>,
    pub(super) gpu: crate::gpu::GpuContext,
    pub(super) effects: EffectRegistry,
    pub(super) global_bg: wgpu::BindGroup,
    pub(super) post_chain: PostProcessChain,
    pub(super) effect_tex: wgpu::Texture,
    pub(super) effect_view: wgpu::TextureView,
    /// Effect + post render resolution. Equal to `gpu.size` when
    /// `config.render_scale == 1.0`, otherwise a fraction of it.
    /// The swapchain always stays at window size; only the effect
    /// and post-chain textures are scaled. The blit pass upscales
    /// internal → swapchain with a CAS-lite sharpen.
    pub(super) internal_size: (u32, u32),
    // Blit pipeline: copies post result (Rgba16Float) to swapchain (sRGB)
    // with optional CAS-lite contrast-adaptive sharpen.
    pub(super) blit_pipeline: wgpu::RenderPipeline,
    pub(super) blit_bgl: wgpu::BindGroupLayout,
    /// Sampler used by the blit pass to sample the post chain's final
    /// texture. Linear filtering so the upscale is smooth before
    /// sharpening. Allocated once; never changes.
    pub(super) blit_sampler: wgpu::Sampler,
    /// Pre-built bind group for the blit pass. Rebuilt only when
    /// `post_chain.final_view()` identity changes (window resize or
    /// render_scale hot-reload).
    pub(super) blit_bg: wgpu::BindGroup,
    /// Uniform buffer feeding `BlitUniforms` to the CAS-lite fragment
    /// shader (inv_src_size + sharpen amount). Rewritten on init,
    /// resize, and render_scale / upscale_sharpen hot-reload.
    pub(super) blit_uniform_buf: wgpu::Buffer,

    pub(super) name_overlay: NameOverlay,
    pub(super) hud: HudOverlay,
    pub(super) audio_picker: Option<AudioPicker>,
    pub(super) audio_picker_overlay: AudioPickerOverlay,
    pub(super) text_input_overlay: TextInputOverlay,
    pub(super) text_input_buffer: Option<String>,
    pub(super) params_overlay: ParamsOverlay,
    pub(super) params_edit: Option<ParamEditState>,
    pub(super) effects_menu_overlay: EffectsMenuOverlay,
    pub(super) effects_menu: Option<EffectsMenuState>,
    pub(super) global_settings_overlay: GlobalSettingsOverlay,
    pub(super) global_settings: Option<GlobalSettingsState>,
    pub(super) vje_overlay: VjeOverlay,
    pub(super) vje_state: Option<VjeOverlayState>,
    pub(super) input: InputHandler,
    pub(super) scene: SceneManager,
    pub(super) beat_tracker: BeatTracker,
    pub(super) audio_state: Arc<AtomicAudioState>,
    pub(super) _audio_stream: Option<crate::audio::capture::AudioStreamHandle>,
    pub(super) config_watcher: Option<crate::config::ConfigWatcher>,
    pub(super) config: Config,

    // Timing
    pub(super) start: Instant,
    pub(super) last_frame: Instant,
    pub(super) last_beat_t: f64,
    pub(super) pulse: f32,
    pub(super) rotation_angle: f32,
    pub(super) vibration_y: f32,
    pub(super) strobe_alpha: f32,
    pub(super) sensitivity: f32,

    // Perf stats (logged every 300 frames at RUST_LOG=info)
    pub(super) frame_count: u32,
    pub(super) perf_window_start: Instant,
    pub(super) frame_times_ms: Vec<f32>,

    /// Set by `window_event` when an E (params) or G (global) overlay
    /// input is dispatched; consumed (logged + cleared) at the end of
    /// the next `render_frame`. Measures the handler→paint interval so
    /// the pingo test ladder can confirm the T4 fix actually dropped
    /// overlay input latency below ~1 frame time.
    pub(super) pending_overlay_input: Option<(&'static str, Instant)>,
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
        );
        let global_bg = effects.global_bind_group(&gpu.device);

        // Effect + post chain render at internal_size, which may be
        // smaller than the swapchain when render_scale < 1.0. The blit
        // pass handles the upscale.
        let internal_size = crate::gpu::internal_size(config.render_scale, gpu.size);
        if internal_size != gpu.size {
            log::info!(
                "render_scale={:.2}: effect+post at {}x{}, swapchain at {}x{}",
                config.render_scale,
                internal_size.0, internal_size.1,
                gpu.size.0, gpu.size.1
            );
        }

        // Effect render target
        let (effect_tex, effect_view) = gpu.create_linear_texture_sized(
            "effect_output", internal_size.0, internal_size.1);

        // Post-processing — pass `effect_view` so the chain can cache the
        // trail-pass bind group once at construction instead of rebuilding
        // it every frame.
        let post_chain = PostProcessChain::new(
            &gpu,
            &effects.global_uniform_buffer,
            &effect_view,
            internal_size,
        );

        // Blit pipeline: copies Rgba16Float → swapchain sRGB with CAS-lite
        // sharpen. Binds a uniform buffer carrying (inv_src_size, sharpen).
        let blit_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit_bgl"),
            entries: &[
                blit_tex_entry(0),
                blit_sampler_entry(1),
                blit_uniform_entry(2),
            ],
        });
        let blit_pipeline = make_blit_pipeline(&gpu.device, &blit_bgl, gpu.surface_format());
        // Linear sampler so the upsample is bilinear underneath the CAS
        // sharpening. Allocated once.
        let blit_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit_sampler_linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let blit_uniform_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit_uniform"),
            size: std::mem::size_of::<BlitUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(
            &blit_uniform_buf,
            0,
            bytemuck::bytes_of(&BlitUniforms::from_sizes(internal_size, config.upscale_sharpen)),
        );
        let blit_bg = make_blit_bind_group(
            &gpu.device,
            &blit_bgl,
            post_chain.final_view(),
            &blit_sampler,
            &blit_uniform_buf,
        );

        let screen_size = gpu.size;

        // Scene manager
        let effect_names = effects.effect_names().iter().map(|s| s.to_string()).collect();
        let scene = SceneManager::new(
            effect_names,
            &config.mirror_pool,
            config.scene_duration,
            config.mirror_cycle_interval,
            config.disabled_effects.as_deref(),
        );

        // Audio (optional — runs silently if no device is available)
        let audio_state = Arc::new(AtomicAudioState::new());
        let stream = match crate::audio::capture::start_capture(
            config.audio_device.as_deref(),
            audio_state.clone(),
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("Audio unavailable: {}. Running silently.", e);
                None
            }
        };

        let config_watcher = crate::config::ConfigWatcher::new(&self.config_path());
        if config_watcher.is_none() {
            log::warn!("ConfigWatcher: not attached — live reload disabled");
        }

        let name_overlay = NameOverlay::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format(),
            screen_size,
            &config.dj_name,
        );

        let hud = HudOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let audio_picker_overlay = AudioPickerOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let text_input_overlay = TextInputOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let params_overlay = ParamsOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let effects_menu_overlay = EffectsMenuOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let global_settings_overlay = GlobalSettingsOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());
        let vje_overlay = VjeOverlay::new(&gpu.device, &gpu.queue, gpu.surface_format());

        let mut beat_tracker = BeatTracker::new(config.beat_sensitivity);
        beat_tracker.set_bpm_lock_range(config.bpm_lock_min, config.bpm_lock_max);

        self.state = Some(AppState {
            window,
            gpu,
            effects,
            global_bg,
            post_chain,
            effect_tex,
            effect_view,
            internal_size,
            blit_pipeline,
            blit_bgl,
            blit_sampler,
            blit_bg,
            blit_uniform_buf,
            name_overlay,
            hud,
            audio_picker: None,
            audio_picker_overlay,
            text_input_overlay,
            text_input_buffer: None,
            params_overlay,
            params_edit: None,
            effects_menu_overlay,
            effects_menu: None,
            global_settings_overlay,
            global_settings: None,
            vje_overlay,
            vje_state: None,
            input: InputHandler::new(),
            scene,
            beat_tracker,
            audio_state,
            _audio_stream: stream, // kept alive to prevent drop/stop
            config_watcher,
            config: self.config.clone(),
            start: Instant::now(),
            last_frame: Instant::now(),
            last_beat_t: 0.0,
            pulse: 0.0,
            rotation_angle: 0.0,
            vibration_y: 0.0,
            strobe_alpha: 0.0,
            sensitivity: self.config.beat_sensitivity,
            frame_count: 0,
            perf_window_start: Instant::now(),
            frame_times_ms: Vec::with_capacity(300),
            pending_overlay_input: None,
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
            WindowEvent::ModifiersChanged(mods) => {
                state.input.shift_held = mods.state().shift_key();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                // While the text-input overlay is open, consume KeyEvents
                // directly as editing input rather than letting the normal
                // input handler interpret them.
                if state.input.text_input_open {
                    handle_text_input_key(state, &key_event);
                    return;
                }
                if let Some(action) = state.input.handle(&key_event) {
                    // T4 instrumentation: tag E (params) / G (global)
                    // overlay inputs with a timestamp so `frame::render_frame`
                    // can log the handler→paint interval on the next frame.
                    let overlay_tag: Option<&'static str> = match &action {
                        Action::ToggleParamEditor
                        | Action::ParamEditUp
                        | Action::ParamEditDown
                        | Action::ParamEditLeft(_)
                        | Action::ParamEditRight(_)
                        | Action::ParamEditConfirm
                        | Action::ParamEditCancel => Some("E"),
                        Action::ToggleGlobalSettings
                        | Action::GlobalSettingsUp
                        | Action::GlobalSettingsDown
                        | Action::GlobalSettingsLeft(_)
                        | Action::GlobalSettingsRight(_)
                        | Action::GlobalSettingsConfirm
                        | Action::GlobalSettingsCancel => Some("G"),
                        _ => None,
                    };
                    if let Some(tag) = overlay_tag {
                        state.pending_overlay_input = Some((tag, Instant::now()));
                    }
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
                        Action::ToggleTextInput => {
                            // Open the text editor pre-populated with the current name.
                            state.text_input_buffer = Some(state.config.dj_name.clone());
                            state.input.text_input_open = true;
                        }
                        Action::ToggleAudioPicker => {
                            if state.audio_picker.is_some() {
                                state.audio_picker = None;
                                state.input.picker_open = false;
                            } else {
                                let active = state.config.audio_device.as_deref();
                                state.audio_picker = Some(AudioPicker::open(active));
                                state.input.picker_open = true;
                            }
                        }
                        Action::PickerUp => {
                            if let Some(p) = &mut state.audio_picker {
                                p.state.move_up();
                            }
                        }
                        Action::PickerDown => {
                            if let Some(p) = &mut state.audio_picker {
                                p.state.move_down();
                            }
                        }
                        Action::PickerJump(n) => {
                            if let Some(p) = &mut state.audio_picker {
                                p.state.jump_to_1indexed(n);
                            }
                        }
                        Action::PickerCancel => {
                            state.audio_picker = None;
                            state.input.picker_open = false;
                        }
                        Action::ToggleParamEditor => {
                            if state.params_edit.is_some() {
                                state.params_edit = None;
                                state.input.param_editor_open = false;
                            } else {
                                let cur = state.scene.current_effect().to_string();
                                let live = state.effects.current_params(&cur)
                                    .map(|p| p.params)
                                    .unwrap_or([0.0; 16]);
                                // Always opens — the overlay shows an
                                // info screen for effects with no
                                // editable params (e.g. mandelbrot_zoom).
                                state.params_edit = Some(ParamEditState::open(&cur, &live));
                                state.input.param_editor_open = true;
                            }
                        }
                        Action::ParamEditUp => {
                            if let Some(ed) = &mut state.params_edit { ed.select_up(); }
                        }
                        Action::ParamEditDown => {
                            if let Some(ed) = &mut state.params_edit { ed.select_down(); }
                        }
                        Action::ParamEditLeft(fast) => {
                            if let Some(ed) = &mut state.params_edit {
                                if ed.has_params() {
                                    ed.nudge(-1, fast);
                                    let mut params = crate::gpu::EffectUniforms {
                                        params: ed.as_params_array(),
                                        seed: 0.0, _pad: [0.0; 3],
                                    };
                                    if let Some(cur) = state.effects.current_params(&ed.effect) {
                                        params.seed = cur.seed;
                                    }
                                    let name = ed.effect.clone();
                                    state.effects.update_effect_params(&state.gpu.queue, &name, &params);
                                }
                            }
                        }
                        Action::ParamEditRight(fast) => {
                            if let Some(ed) = &mut state.params_edit {
                                if ed.has_params() {
                                    ed.nudge(1, fast);
                                    let mut params = crate::gpu::EffectUniforms {
                                        params: ed.as_params_array(),
                                        seed: 0.0, _pad: [0.0; 3],
                                    };
                                    if let Some(cur) = state.effects.current_params(&ed.effect) {
                                        params.seed = cur.seed;
                                    }
                                    let name = ed.effect.clone();
                                    state.effects.update_effect_params(&state.gpu.queue, &name, &params);
                                }
                            }
                        }
                        Action::ParamEditConfirm => {
                            if let Some(ed) = state.params_edit.take() {
                                if ed.has_params() {
                                    // Persist each changed param to repo config.json
                                    for (i, def) in ed.defs.iter().enumerate() {
                                        let v = ed.values[i];
                                        if let Err(e) = crate::config::write_fx_param(
                                            &crate::config::dirs_config(), &ed.effect, def.name, v,
                                        ) {
                                            log::warn!("Could not persist {}.{}: {}", ed.effect, def.name, e);
                                        }
                                        // Mirror into in-memory config so hot-reload doesn't undo us
                                        state.config.fx_params
                                            .entry(ed.effect.clone())
                                            .or_default()
                                            .insert(def.name.to_string(), serde_json::json!(v as f64));
                                    }
                                    log::info!("Saved {} params to XDG config", ed.effect);
                                }
                                state.input.param_editor_open = false;
                            }
                        }
                        Action::ParamEditCancel => {
                            if let Some(mut ed) = state.params_edit.take() {
                                if ed.has_params() {
                                    // Restore live values to the entry-time snapshot
                                    ed.restore_original();
                                    let params = crate::gpu::EffectUniforms {
                                        params: ed.as_params_array(),
                                        seed: state.effects.current_params(&ed.effect).map(|p| p.seed).unwrap_or(0.0),
                                        _pad: [0.0; 3],
                                    };
                                    let name = ed.effect.clone();
                                    state.effects.update_effect_params(&state.gpu.queue, &name, &params);
                                }
                                state.input.param_editor_open = false;
                            }
                        }
                        Action::ToggleGlobalSettings => {
                            if state.global_settings.is_some() {
                                state.global_settings = None;
                                state.input.global_settings_open = false;
                            } else {
                                state.global_settings = Some(GlobalSettingsState::open(&state.config));
                                state.input.global_settings_open = true;
                            }
                        }
                        Action::GlobalSettingsUp => {
                            if let Some(g) = &mut state.global_settings { g.select_up(); }
                        }
                        Action::GlobalSettingsDown => {
                            if let Some(g) = &mut state.global_settings { g.select_down(); }
                        }
                        Action::GlobalSettingsLeft(fast) => {
                            if let Some(g) = &mut state.global_settings {
                                g.nudge(&mut state.config, -1, fast);
                            }
                        }
                        Action::GlobalSettingsRight(fast) => {
                            if let Some(g) = &mut state.global_settings {
                                g.nudge(&mut state.config, 1, fast);
                            }
                        }
                        Action::GlobalSettingsConfirm => {
                            if state.global_settings.take().is_some() {
                                // Persist all eight knobs in one atomic write.
                                let updates: Vec<(&str, serde_json::Value)> =
                                    GlobalKnob::ALL
                                        .iter()
                                        .map(|k| (k.config_key(), k.to_json(&state.config)))
                                        .collect();
                                if let Err(e) = crate::config::write_xdg_fields(&updates) {
                                    log::warn!("Could not persist global settings: {}", e);
                                } else {
                                    log::info!("Saved global settings to XDG config");
                                }
                                state.input.global_settings_open = false;
                            }
                        }
                        Action::GlobalSettingsCancel => {
                            if let Some(g) = state.global_settings.take() {
                                g.restore(&mut state.config);
                                state.input.global_settings_open = false;
                            }
                        }
                        Action::SceneDurationDown => {
                            let new_dur = (state.scene.scene_duration() - 5.0).max(3.0);
                            state.scene.set_scene_duration(new_dur);
                            state.config.scene_duration = state.scene.scene_duration();
                            if let Err(e) = crate::config::write_scene_duration(state.scene.scene_duration()) {
                                log::warn!("Could not persist scene_duration: {}", e);
                            }
                            log::info!("Scene duration: {:.0}s", state.scene.scene_duration());
                        }
                        Action::SceneDurationUp => {
                            let new_dur = (state.scene.scene_duration() + 5.0).min(300.0);
                            state.scene.set_scene_duration(new_dur);
                            state.config.scene_duration = state.scene.scene_duration();
                            if let Err(e) = crate::config::write_scene_duration(state.scene.scene_duration()) {
                                log::warn!("Could not persist scene_duration: {}", e);
                            }
                            log::info!("Scene duration: {:.0}s", state.scene.scene_duration());
                        }
                        Action::ToggleEffectsMenu => {
                            if state.effects_menu.is_some() {
                                state.effects_menu = None;
                                state.input.effects_menu_open = false;
                            } else {
                                let names: Vec<String> = state.scene.effect_names().to_vec();
                                let mask: Vec<bool> = state.scene.enabled().to_vec();
                                state.effects_menu = Some(EffectsMenuState::open(&names, &mask));
                                state.input.effects_menu_open = true;
                            }
                        }
                        Action::EffectsMenuUp => {
                            if let Some(m) = &mut state.effects_menu { m.move_up(); }
                        }
                        Action::EffectsMenuDown => {
                            if let Some(m) = &mut state.effects_menu { m.move_down(); }
                        }
                        Action::EffectsMenuToggle => {
                            if let Some(m) = &mut state.effects_menu { m.toggle_current(); }
                        }
                        Action::EffectsMenuConfirm => {
                            if let Some(m) = state.effects_menu.take() {
                                // Persist as a deny list so future code
                                // updates that add new effects auto-enable.
                                let disabled_list = m.disabled_names();
                                let opt: Option<&[String]> = if disabled_list.is_empty() {
                                    None
                                } else {
                                    Some(disabled_list.as_slice())
                                };
                                state.scene.set_disabled_filter(opt);
                                state.config.disabled_effects = if disabled_list.is_empty() {
                                    None
                                } else {
                                    Some(disabled_list.clone())
                                };
                                if let Err(e) = crate::config::write_disabled_effects(opt) {
                                    log::warn!("Could not persist disabled_effects: {}", e);
                                }
                                log::info!(
                                    "Effects: {}/{} on ({} disabled)",
                                    m.effect_names.len() - disabled_list.len(),
                                    m.effect_names.len(),
                                    disabled_list.len(),
                                );
                                state.input.effects_menu_open = false;
                            }
                        }
                        Action::EffectsMenuCancel => {
                            state.effects_menu = None;
                            state.input.effects_menu_open = false;
                        }
                        Action::ToggleVjeOverlay => {
                            if state.vje_state.is_some() {
                                state.vje_state = None;
                                state.input.vje_open = false;
                            } else {
                                state.vje_state = Some(VjeOverlayState::open(&state.config));
                                state.input.vje_open = true;
                            }
                        }
                        Action::VjeUp => {
                            if let Some(st) = &mut state.vje_state {
                                match st.tab {
                                    VjeTab::Effects => match st.effects_focus {
                                        VjeEffectsFocus::List   => st.effect_list_up(),
                                        VjeEffectsFocus::Params => st.param_up(),
                                    },
                                    VjeTab::Globals => st.global_up(),
                                }
                            }
                        }
                        Action::VjeDown => {
                            if let Some(st) = &mut state.vje_state {
                                match st.tab {
                                    VjeTab::Effects => match st.effects_focus {
                                        VjeEffectsFocus::List   => st.effect_list_down(),
                                        VjeEffectsFocus::Params => st.param_down(),
                                    },
                                    VjeTab::Globals => st.global_down(),
                                }
                            }
                        }
                        Action::VjeLeft(fast) => {
                            if let Some(st) = &mut state.vje_state {
                                match st.tab {
                                    VjeTab::Effects => {
                                        if matches!(st.effects_focus, VjeEffectsFocus::Params) {
                                            st.nudge_current_param(&mut state.config, -1, fast);
                                        }
                                    }
                                    VjeTab::Globals => {
                                        st.nudge_global(&mut state.config, -1, fast);
                                    }
                                }
                            }
                        }
                        Action::VjeRight(fast) => {
                            if let Some(st) = &mut state.vje_state {
                                match st.tab {
                                    VjeTab::Effects => {
                                        if matches!(st.effects_focus, VjeEffectsFocus::Params) {
                                            st.nudge_current_param(&mut state.config, 1, fast);
                                        } else if matches!(st.effects_focus, VjeEffectsFocus::List) {
                                            // Right arrow from list focus opens params —
                                            // matches the standalone vje TUI behaviour.
                                            st.focus_params();
                                        }
                                    }
                                    VjeTab::Globals => {
                                        st.nudge_global(&mut state.config, 1, fast);
                                    }
                                }
                            }
                        }
                        Action::VjeTab => {
                            if let Some(st) = &mut state.vje_state { st.switch_tab(); }
                        }
                        Action::VjeFocusSwap => {
                            if let Some(st) = &mut state.vje_state {
                                if matches!(st.tab, VjeTab::Effects) {
                                    st.swap_effects_focus();
                                }
                            }
                        }
                        Action::VjeEnter => {
                            if let Some(st) = &mut state.vje_state {
                                let updates_owned = st.build_updates(&state.config);
                                if updates_owned.is_empty() {
                                    st.set_status("nothing to commit");
                                } else {
                                    // write_xdg_fields takes &[(&str, Value)], so
                                    // borrow the owned String keys.
                                    let updates_ref: Vec<(&str, serde_json::Value)> =
                                        updates_owned.iter()
                                            .map(|(k, v)| (k.as_str(), v.clone()))
                                            .collect();
                                    match crate::config::write_xdg_fields(&updates_ref) {
                                        Ok(()) => {
                                            let n = updates_ref.len();
                                            st.mark_committed();
                                            st.set_status(format!("committed {} field(s)", n));
                                            log::info!("vje overlay: committed {} fields", n);
                                        }
                                        Err(e) => {
                                            st.set_status(format!("commit failed: {}", e));
                                            log::warn!("vje overlay: commit failed: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Action::VjeEsc => {
                            // Esc inside params → back to list; from list →
                            // close the overlay (mirrors vje TUI behaviour).
                            if let Some(st) = &mut state.vje_state {
                                let close = match st.tab {
                                    VjeTab::Effects => matches!(st.effects_focus, VjeEffectsFocus::List),
                                    VjeTab::Globals => true,
                                };
                                if close {
                                    state.vje_state = None;
                                    state.input.vje_open = false;
                                } else if let Some(st) = &mut state.vje_state {
                                    st.focus_list();
                                }
                            }
                        }
                        Action::VjeReset => {
                            if let Some(st) = &mut state.vje_state {
                                match st.tab {
                                    VjeTab::Effects => {
                                        if matches!(st.effects_focus, VjeEffectsFocus::Params) {
                                            st.reset_current_param(&mut state.config);
                                        }
                                    }
                                    VjeTab::Globals => st.reset_global(&mut state.config),
                                }
                            }
                        }
                        Action::VjeToggleDisable => {
                            if let Some(st) = &mut state.vje_state {
                                if matches!(st.tab, VjeTab::Effects)
                                    && matches!(st.effects_focus, VjeEffectsFocus::List)
                                {
                                    st.toggle_disabled(&mut state.config);
                                    // Also update the scene's live filter so
                                    // the change takes effect for the preview
                                    // without waiting for the commit+watcher.
                                    state.scene.set_disabled_filter(
                                        state.config.disabled_effects.as_deref(),
                                    );
                                }
                            }
                        }
                        Action::PickerConfirm => {
                            // Extract name before dropping picker (releases borrow)
                            let name = state
                                .audio_picker
                                .as_ref()
                                .and_then(|p| p.state.selected_name().map(|s| s.to_string()));

                            // Close picker and scanner
                            state.audio_picker = None;
                            state.input.picker_open = false;

                            if let Some(name) = name {
                                // Drop old stream before opening new one
                                state._audio_stream = None;
                                match crate::audio::capture::start_capture(
                                    Some(&name),
                                    state.audio_state.clone(),
                                ) {
                                    Ok(s) => {
                                        log::info!("Switched audio device to '{}'", name);
                                        state._audio_stream = Some(s);
                                        state.config.audio_device = Some(name.clone());
                                        if let Err(e) = crate::config::write_audio_device(&name) {
                                            log::warn!("Could not save audio_device to config: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("Could not open device '{}': {}", name, e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                let new_size = (size.width.max(1), size.height.max(1));
                let s = &mut self.state.as_mut().unwrap();
                s.gpu.resize(new_size);
                rebuild_render_targets(s);
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
        self.config_path.clone()
    }

    /// Dispatch to `frame::render_frame` once state has been
    /// initialized by `resumed()`. See `src/app/frame.rs` for the
    /// full per-frame pipeline.
    fn render_frame(&mut self) {
        if let Some(state) = self.state.as_mut() {
            frame::render_frame(state);
        }
    }
}

fn handle_text_input_key(state: &mut AppState, ev: &winit::event::KeyEvent) {
    use winit::event::ElementState;
    use winit::keyboard::{Key, NamedKey};

    if ev.state != ElementState::Pressed {
        return;
    }

    match &ev.logical_key {
        Key::Named(NamedKey::Escape) => {
            // Cancel — discard edits.
            state.text_input_buffer = None;
            state.input.text_input_open = false;
            return;
        }
        Key::Named(NamedKey::Enter) => {
            if let Some(new_name) = state.text_input_buffer.take() {
                let trimmed = new_name.trim();
                if !trimmed.is_empty() {
                    state.name_overlay.set_name(trimmed);
                    state.config.dj_name = trimmed.to_string();
                    if let Err(e) = crate::config::write_dj_name(trimmed) {
                        log::warn!("Could not persist dj_name to config: {}", e);
                    } else {
                        log::info!("DJ name updated to '{}'", trimmed);
                    }
                }
            }
            state.input.text_input_open = false;
            return;
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(buf) = state.text_input_buffer.as_mut() {
                buf.pop();
            }
            return;
        }
        _ => {}
    }

    // Append any printable text associated with this key (respects keyboard
    // layout, Shift, etc.). Ignores control characters.
    if let Some(text) = ev.text.as_ref() {
        if let Some(buf) = state.text_input_buffer.as_mut() {
            for ch in text.chars() {
                if !ch.is_control() {
                    buf.push(ch);
                }
            }
        }
    }
}

// Blit pipeline + CAS-lite upscale shader live in `blit.rs`.
