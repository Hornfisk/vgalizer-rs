//! Per-frame render pipeline.
//!
//! `render_frame` is the hot path: read audio state, hot-reload config,
//! detect beats, update scene + timers, build uniforms, run the effect
//! + post-process + blit passes, render overlays, present. Extracted
//! from `app.rs` into its own module so the 400-LOC render loop can be
//! read in isolation from the lifecycle / event routing code.
//!
//! All state lives on `AppState`; `render_frame` takes it as
//! `&mut AppState` and mutates in place. Caller (in `mod.rs`) does the
//! `Option<AppState>` unwrapping.

use std::time::Instant;

use rand::Rng;

use crate::colors::palette;
use crate::gpu::uniforms::pack_bands;
use crate::gpu::{GlobalUniforms, PostUniforms};

use super::blit::{rebuild_render_targets, BlitUniforms};
use super::AppState;

/// Execute one full frame. Called from `App::render_frame` once per
/// `RedrawRequested` event.
pub(super) fn render_frame(state: &mut AppState) {
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
            if (new_cfg.bpm_lock_min - state.config.bpm_lock_min).abs() > 0.01
                || (new_cfg.bpm_lock_max - state.config.bpm_lock_max).abs() > 0.01
            {
                log::info!(
                    "reload: bpm_lock {}..{} -> {}..{}",
                    state.config.bpm_lock_min, state.config.bpm_lock_max,
                    new_cfg.bpm_lock_min, new_cfg.bpm_lock_max
                );
                state
                    .beat_tracker
                    .set_bpm_lock_range(new_cfg.bpm_lock_min, new_cfg.bpm_lock_max);
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
            if (new_cfg.mirror_cycle_interval - state.config.mirror_cycle_interval).abs() > 0.001 {
                log::info!(
                    "reload: mirror_cycle_interval {} -> {}",
                    state.config.mirror_cycle_interval, new_cfg.mirror_cycle_interval
                );
                state.scene.set_mirror_cycle_interval(new_cfg.mirror_cycle_interval);
            }
            if new_cfg.disabled_effects != state.config.disabled_effects {
                log::info!("reload: disabled_effects -> {:?}", new_cfg.disabled_effects);
                state.scene.set_disabled_filter(new_cfg.disabled_effects.as_deref());
            }
            let fx_changed = new_cfg.fx_params != state.config.fx_params;
            let render_scale_changed =
                (new_cfg.render_scale - state.config.render_scale).abs() > 0.001;
            let sharpen_changed =
                (new_cfg.upscale_sharpen - state.config.upscale_sharpen).abs() > 0.001;
            state.config = new_cfg;
            // Handle render_scale / upscale_sharpen after the config
            // swap so the rebuild helpers read the new values.
            if render_scale_changed {
                log::info!("reload: render_scale -> {:.2}", state.config.render_scale);
                rebuild_render_targets(state);
            } else if sharpen_changed {
                // Sharpen-only change: no texture rebuild needed, just
                // rewrite the blit uniform in place.
                log::info!("reload: upscale_sharpen -> {:.2}", state.config.upscale_sharpen);
                state.gpu.queue.write_buffer(
                    &state.blit_uniform_buf,
                    0,
                    bytemuck::bytes_of(&BlitUniforms::from_sizes(
                        state.internal_size,
                        state.config.upscale_sharpen,
                    )),
                );
            }
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
        // Sort in place; clear afterwards. Avoids a Vec::clone() per
        // perf window. NaN-safe: if a dt is ever NaN (clock hiccup),
        // treat it as equal so we don't panic 4 hours into a set.
        state.frame_times_ms
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50 = state.frame_times_ms[state.frame_times_ms.len() / 2];
        let p99 = state.frame_times_ms[(state.frame_times_ms.len() * 99) / 100];
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
    let kick_flux = state.audio_state.load_kick_flux();

    // Beat detection — flux path is sharp, RMS path is the legacy
    // fallback toggled via `beat_source` in the XDG config so a
    // problematic track can be recovered mid-set without a restart.
    let beat_input = if state.config.beat_source == "rms" {
        level
    } else {
        kick_flux
    };
    let beat_state = state.beat_tracker.update(beat_input, t);

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
        // Shaders use `resolution` for pixel-space math. When
        // render_scale < 1.0 the effect + post chain are rendered
        // at `internal_size`, so we report *that* size, not the
        // swapchain size. This keeps effect visuals identical
        // regardless of scale (the CAS blit handles upscaling).
        resolution: [state.internal_size.0 as f32, state.internal_size.1 as f32],
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
    // Bind groups are pre-built; this is allocation-free.
    state.post_chain.process(&mut encoder);

    // --- Compute viz + panel rects ---
    //
    // When the unified vje overlay is open, shrink the blit viewport
    // to a right-side preview rect and reserve the left for the
    // editor panel. The effect rendering + post chain stay at full
    // resolution; only the final blit-to-swapchain is scissored.
    // Proportional layout so it looks reasonable at any surface
    // size, not just pingo's 1366×768.
    let (sw, sh) = state.gpu.size;
    let vje_open = state.vje_state.is_some();
    let (vx, vy, vw, vh, panel_rect) = if vje_open {
        let sw_f = sw as f32;
        let sh_f = sh as f32;
        // Panel takes left 48% of the surface, preview gets the rest
        // minus a small gutter on each side.
        let gutter = 16.0;
        let panel_w = (sw_f * 0.48).floor();
        let preview_col_x = panel_w + gutter;
        let preview_col_w = (sw_f - preview_col_x - gutter).max(1.0);
        // Preview is 16:9, fit inside the available column, centered
        // vertically.
        let (pw, ph) = fit_aspect(preview_col_w, sh_f - gutter * 2.0, 16.0 / 9.0);
        let px = preview_col_x + (preview_col_w - pw) * 0.5;
        let py = (sh_f - ph) * 0.5;
        (px, py, pw, ph, (0.0, 0.0, panel_w, sh_f))
    } else {
        (0.0, 0.0, sw as f32, sh as f32, (0.0, 0.0, 0.0, 0.0))
    };

    // --- Blit to swapchain ---
    // blit_bg is pre-built and references post_chain.final_view(),
    // which is stable until the next window resize.
    {
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
        pass.set_bind_group(0, &state.blit_bg, &[]);
        // Scissor the viz to the preview sub-rect when the vje
        // overlay is open. When it isn't, the viewport is the whole
        // surface so the viz fills the window as before.
        pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
        pass.draw(0..3, 0..1);
    }

    // --- Name overlay (rendered directly to swapchain) ---
    // Suppressed while the vje overlay is open — the DJ name in the
    // top-left would collide with the panel text. The overlay has
    // its own title bar anyway.
    if !vje_open {
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
    }

    // --- HUD overlay ---
    // Also suppressed while vje is open. The vje overlay carries its
    // own status line, and the HUD text would overdraw the panel.
    if !vje_open {
        state.hud.render(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &output_view,
            state.gpu.size,
        );
    }

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

    // --- Unified vje overlay (only when open) ---
    // Rendered last so it draws on top of everything else, and uses
    // the precomputed panel_rect so the layout is locked to the
    // same proportional split that the blit viewport used above.
    if let Some(st) = state.vje_state.as_mut() {
        state.vje_overlay.update_text(st, &state.config);
        state.vje_overlay.render(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &output_view,
            state.gpu.size,
            panel_rect,
        );
    }

    state.gpu.queue.submit([encoder.finish()]);
    output.present();
}

/// Fit a (width, height) rect with the given aspect ratio inside
/// `(max_w, max_h)`, returning the largest rect that doesn't overflow.
/// Used to letterbox the viz preview into whatever column is free beside
/// the vje overlay panel.
fn fit_aspect(max_w: f32, max_h: f32, aspect: f32) -> (f32, f32) {
    let from_w = (max_w, max_w / aspect);
    let from_h = (max_h * aspect, max_h);
    if from_w.1 <= max_h { from_w } else { from_h }
}
