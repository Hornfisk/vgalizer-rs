use std::sync::Arc;
use std::time::Instant;

use rand::Rng;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use crate::audio::{AtomicAudioState, BeatTracker};
use crate::audio_picker::{AudioPicker, AudioPickerOverlay};
use crate::effects_menu::{EffectsMenuOverlay, EffectsMenuState};
use crate::global_settings::{GlobalKnob, GlobalSettingsOverlay, GlobalSettingsState};
use crate::colors::palette;
use crate::config::Config;
use crate::effects::{manager::SceneManager, EffectRegistry};
use crate::gpu::{GlobalUniforms, PostUniforms};
use crate::gpu::uniforms::pack_bands;
use crate::input::{Action, InputHandler};
use crate::overlay::HudOverlay;
use crate::postprocess::PostProcessChain;
use crate::text::{NameOverlay, ParamEditState, ParamsOverlay, TextInputOverlay};

pub fn run(config: Config, config_path: String) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App { config, config_path, state: None };
    event_loop.run_app(&mut app).expect("Event loop failed");
}

struct App {
    config: Config,
    config_path: String,
    state: Option<AppState>,
}

#[allow(dead_code)]
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
    audio_picker: Option<AudioPicker>,
    audio_picker_overlay: AudioPickerOverlay,
    text_input_overlay: TextInputOverlay,
    text_input_buffer: Option<String>,
    params_overlay: ParamsOverlay,
    params_edit: Option<ParamEditState>,
    effects_menu_overlay: EffectsMenuOverlay,
    effects_menu: Option<EffectsMenuState>,
    global_settings_overlay: GlobalSettingsOverlay,
    global_settings: Option<GlobalSettingsState>,
    input: InputHandler,
    scene: SceneManager,
    beat_tracker: BeatTracker,
    audio_state: Arc<AtomicAudioState>,
    _audio_stream: Option<crate::audio::capture::AudioStreamHandle>,
    config_watcher: Option<crate::config::ConfigWatcher>,
    config: Config,

    // Timing
    start: Instant,
    last_frame: Instant,
    last_beat_t: f64,
    pulse: f32,
    rotation_angle: f32,
    vibration_y: f32,
    strobe_alpha: f32,
    sensitivity: f32,

    // Perf stats (logged every 300 frames at RUST_LOG=info)
    frame_count: u32,
    perf_window_start: Instant,
    frame_times_ms: Vec<f32>,
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
        let scene = SceneManager::new(
            effect_names,
            &config.mirror_pool,
            config.scene_duration,
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
                // Recreate render textures at the new resolution
                let (effect_tex, effect_view) = s.gpu.create_linear_texture("effect_output");
                s.effect_tex  = effect_tex;
                s.effect_view = effect_view;
                s.post_chain  = PostProcessChain::new(&s.gpu, &s.effects.global_uniform_buffer);
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
                // Push owned-by-SceneManager fields to the scene before the
                // config swap, since the scene caches them at construction.
                if (new_cfg.scene_duration - state.config.scene_duration).abs() > 0.001 {
                    log::info!("reload: scene_duration {} -> {}", state.config.scene_duration, new_cfg.scene_duration);
                    state.scene.set_scene_duration(new_cfg.scene_duration);
                }
                if new_cfg.mirror_pool != state.config.mirror_pool {
                    log::info!("reload: mirror_pool changed -> {:?}", new_cfg.mirror_pool);
                    state.scene.set_mirror_pool(&new_cfg.mirror_pool);
                }
                if new_cfg.disabled_effects != state.config.disabled_effects {
                    log::info!("reload: disabled_effects -> {:?}", new_cfg.disabled_effects);
                    state.scene.set_disabled_filter(new_cfg.disabled_effects.as_deref());
                }
                let fx_changed = new_cfg.fx_params != state.config.fx_params;
                state.config = new_cfg;
                // Re-upload params for the active effect if its named knobs
                // changed in the config file (e.g. another machine pushed).
                if fx_changed && state.params_edit.is_none() {
                    let cur = state.scene.current_effect().to_string();
                    if !crate::effects::params::effect_params(&cur).is_empty() {
                        let p = crate::effects::params::effect_uniforms_from_config(
                            &cur, &state.config.fx_params,
                        );
                        state.effects.update_effect_params(&state.gpu.queue, &cur, &p);
                    }
                }
            }
        }

        let now = Instant::now();
        let t = state.start.elapsed().as_secs_f64();
        let dt = state.last_frame.elapsed().as_secs_f64().min(0.05) as f32;
        state.frame_times_ms.push(dt * 1000.0);
        state.frame_count += 1;
        if state.frame_count >= 300 {
            let elapsed = state.perf_window_start.elapsed().as_secs_f32();
            let fps = state.frame_count as f32 / elapsed;
            let mut sorted = state.frame_times_ms.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = sorted[sorted.len() / 2];
            let p99 = sorted[(sorted.len() * 99) / 100];
            log::info!(
                "perf: {:.1} fps  p50={:.2}ms  p99={:.2}ms",
                fps, p50, p99
            );
            state.frame_count = 0;
            state.perf_window_start = now;
            state.frame_times_ms.clear();
        }
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

        // Rotation spring (beat kick, decays back to 0)
        let rot_target = if beat_state.beat { state.config.global_rotation * 0.02 } else { 0.0 };
        state.rotation_angle = state.rotation_angle * 0.95 + rot_target * 0.05;

        // Vibration spring (beat kick, fast decay)
        if beat_state.beat {
            state.vibration_y = state.config.global_vibration * 0.025;
        } else {
            state.vibration_y *= 0.80;
        }

        // Update scene; randomize effect params on switch
        let scene_switched = state.scene.update(&beat_state);

        // Current scene state
        let effect_name = state.scene.current_effect().to_string();

        if scene_switched {
            // If the effect has named params, load them from config (with
            // defaults). Otherwise fall back to randomised params for the
            // existing v1/v2 effects that don't expose named knobs.
            let defs = crate::effects::params::effect_params(&effect_name);
            let params = if defs.is_empty() {
                let mut rng = rand::thread_rng();
                crate::gpu::EffectUniforms {
                    params: std::array::from_fn(|_| rng.gen::<f32>()),
                    seed: rng.gen::<f32>(),
                    _pad: [0.0; 3],
                }
            } else {
                crate::effects::params::effect_uniforms_from_config(
                    &effect_name,
                    &state.config.fx_params,
                )
            };
            state.effects.update_effect_params(&state.gpu.queue, &effect_name, &params);
        }
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
            vibration_y: state.vibration_y,
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
        state.hud.update_text(
            &effect_name,
            beat_state.bpm,
            state.sensitivity,
            level,
            state.scene.scene_duration(),
        );

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

        // --- Param editor overlay (only when open) ---
        if let Some(ed) = &state.params_edit {
            state.params_overlay.update_text(ed);
            state.params_overlay.render(
                &state.gpu.device,
                &state.gpu.queue,
                &mut encoder,
                &output_view,
                state.gpu.size,
            );
        }

        // --- Effects menu overlay (only when open) ---
        if let Some(menu) = &state.effects_menu {
            state.effects_menu_overlay.update_text(menu, state.scene.scene_duration());
            state.effects_menu_overlay.render(
                &state.gpu.device,
                &state.gpu.queue,
                &mut encoder,
                &output_view,
                state.gpu.size,
            );
        }

        // --- Global settings overlay (only when open) ---
        if let Some(g) = &state.global_settings {
            state.global_settings_overlay.update_text(g, &state.config);
            state.global_settings_overlay.render(
                &state.gpu.device,
                &state.gpu.queue,
                &mut encoder,
                &output_view,
                state.gpu.size,
            );
        }

        // --- Audio picker overlay (only when open) ---
        if let Some(picker) = &mut state.audio_picker {
            picker.tick();
            state.audio_picker_overlay.update(picker);
            state.audio_picker_overlay.render(
                &state.gpu.device,
                &state.gpu.queue,
                &mut encoder,
                &output_view,
                state.gpu.size,
            );
        }

        // --- Text input overlay (only when open) ---
        if let Some(buf) = &state.text_input_buffer {
            state.text_input_overlay.tick(dt);
            state.text_input_overlay.update_text(buf);
            state.text_input_overlay.render(
                &state.gpu.device,
                &state.gpu.queue,
                &mut encoder,
                &output_view,
                state.gpu.size,
            );
        }

        state.gpu.queue.submit([encoder.finish()]);
        output.present();
    }
}

/// Process a raw KeyEvent while the DJ-name text-input overlay is open.
/// Handles character entry (via KeyEvent.text), Backspace, Enter (commit),
/// and Escape (cancel). On commit, the new name is pushed to the live
/// NameOverlay and persisted to the XDG config file.
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
