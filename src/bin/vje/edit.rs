//! Mutation helpers for vje: nudge per-effect params, nudge / cycle globals,
//! reset to default, and atomic commit to the XDG config.
//!
//! All edits go through the in-memory `Config` + dirty tracking. `commit_to_disk`
//! is the only function that touches the filesystem, and it uses the existing
//! `write_xdg_fields` helper so the visualizer sees a clean atomic rename.

use crate::state::{AppState, GlobalKind};
use serde_json::json;
use vgalizer::config::{self, Config};
use vgalizer::effects::params::effect_params;

const STROBE_CYCLE: &[&str] = &["off", "beat", "half", "quarter"];

// === Effects tab: per-effect param editing =================================

/// Read the current value of `effect.param_name` from the edited config,
/// falling back to the ParamDef default.
pub fn read_param(cfg: &Config, effect: &str, param_name: &str, default: f32) -> f32 {
    cfg.fx_params
        .get(effect)
        .and_then(|m| m.get(param_name))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn write_param(cfg: &mut Config, effect: &str, param_name: &str, value: f32) {
    let effect_map = cfg
        .fx_params
        .entry(effect.to_string())
        .or_insert_with(Default::default);
    effect_map.insert(param_name.to_string(), json!(value as f64));
}

/// Nudge the currently-hovered param in the Effects tab. `shift` multiplies
/// the step by 10. Value is clamped to the ParamDef range.
pub fn nudge_effect_param(state: &mut AppState, dir: i32, shift: bool) {
    let effect = state.current_effect();
    let params = effect_params(effect);
    if params.is_empty() {
        state.status = format!("{} has no editable params", effect);
        return;
    }
    let Some(def) = params.get(state.param_cursor) else {
        return;
    };
    let cur = read_param(&state.config, effect, def.name, def.default);
    let mult = if shift { 10.0 } else { 1.0 };
    let step = def.step * mult * dir as f32;
    let next = (cur + step).clamp(def.min, def.max);
    write_param(&mut state.config, effect, def.name, next);
    state.dirty_fx_params = true;
    state.status = format!("{}.{} = {:.3}", effect, def.name, next);
}

pub fn reset_effect_param(state: &mut AppState) {
    let effect = state.current_effect();
    let params = effect_params(effect);
    let Some(def) = params.get(state.param_cursor) else {
        return;
    };
    write_param(&mut state.config, effect, def.name, def.default);
    state.dirty_fx_params = true;
    state.status = format!("{}.{} reset -> {:.3}", effect, def.name, def.default);
}

/// Toggle enable/disable of the currently-selected effect (M-menu equivalent).
pub fn toggle_disabled_effect(state: &mut AppState) {
    let effect = state.current_effect().to_string();
    let mut list = state.config.disabled_effects.clone().unwrap_or_default();
    if let Some(pos) = list.iter().position(|s| s == &effect) {
        list.remove(pos);
        state.status = format!("enabled {}", effect);
    } else {
        list.push(effect.clone());
        state.status = format!("disabled {}", effect);
    }
    state.config.disabled_effects = if list.is_empty() { None } else { Some(list) };
    state.dirty_fields.insert("disabled_effects");
}

// === Globals tab editing ====================================================

pub fn nudge_global(state: &mut AppState, dir: i32, shift: bool) {
    let Some(row) = crate::state::GLOBAL_ROWS.get(state.global_cursor) else {
        return;
    };
    if !row.editable {
        return;
    }
    let mult = if shift { 10.0 } else { 1.0 };
    let d = dir as f32 * mult;
    let cfg = &mut state.config;
    let mut dirty_key: Option<&'static str> = None;
    match row.kind {
        GlobalKind::SceneDuration => {
            cfg.scene_duration = (cfg.scene_duration + d as f64 * 1.0).max(1.0);
            dirty_key = Some("scene_duration");
        }
        GlobalKind::BeatSensitivity => {
            cfg.beat_sensitivity = (cfg.beat_sensitivity + d * 0.05).clamp(0.1, 5.0);
            dirty_key = Some("beat_sensitivity");
        }
        GlobalKind::StrobeMode => {
            let idx = STROBE_CYCLE
                .iter()
                .position(|s| *s == cfg.strobe_mode)
                .unwrap_or(1);
            let n = STROBE_CYCLE.len() as i32;
            let next = ((idx as i32 + dir).rem_euclid(n)) as usize;
            cfg.strobe_mode = STROBE_CYCLE[next].to_string();
            dirty_key = Some("strobe_mode");
        }
        GlobalKind::FxSpeedMult => {
            cfg.fx_speed_mult = (cfg.fx_speed_mult + d * 0.05).clamp(0.1, 4.0);
            dirty_key = Some("fx_speed_mult");
        }
        GlobalKind::VgaSync => {
            cfg.vga_sync = (cfg.vga_sync + d * 0.01).clamp(0.0, 1.0);
            dirty_key = Some("vga_sync");
        }
        GlobalKind::MirrorCount => {
            let next = (cfg.mirror_count as i32 + dir * mult as i32).max(1);
            cfg.mirror_count = next as u32;
            dirty_key = Some("mirror_count");
        }
        GlobalKind::MirrorSpread => {
            cfg.mirror_spread += dir * mult as i32;
            dirty_key = Some("mirror_spread");
        }
        GlobalKind::DjName => {
            // Text field: Enter triggers edit mode, not nudge.
        }
        _ => {}
    }
    if let Some(k) = dirty_key {
        state.dirty_fields.insert(k);
        state.status = format!("{} updated", k);
    }
}

pub fn reset_global(state: &mut AppState) {
    let Some(row) = crate::state::GLOBAL_ROWS.get(state.global_cursor) else {
        return;
    };
    if !row.editable {
        return;
    }
    let default = Config::default();
    let cfg = &mut state.config;
    let mut dirty_key: Option<&'static str> = None;
    match row.kind {
        GlobalKind::SceneDuration => {
            cfg.scene_duration = default.scene_duration;
            dirty_key = Some("scene_duration");
        }
        GlobalKind::BeatSensitivity => {
            cfg.beat_sensitivity = default.beat_sensitivity;
            dirty_key = Some("beat_sensitivity");
        }
        GlobalKind::StrobeMode => {
            cfg.strobe_mode = default.strobe_mode;
            dirty_key = Some("strobe_mode");
        }
        GlobalKind::FxSpeedMult => {
            cfg.fx_speed_mult = default.fx_speed_mult;
            dirty_key = Some("fx_speed_mult");
        }
        GlobalKind::VgaSync => {
            cfg.vga_sync = default.vga_sync;
            dirty_key = Some("vga_sync");
        }
        GlobalKind::MirrorCount => {
            cfg.mirror_count = default.mirror_count;
            dirty_key = Some("mirror_count");
        }
        GlobalKind::MirrorSpread => {
            cfg.mirror_spread = default.mirror_spread;
            dirty_key = Some("mirror_spread");
        }
        GlobalKind::DjName => {
            cfg.dj_name = default.dj_name;
            dirty_key = Some("dj_name");
        }
        _ => {}
    }
    if let Some(k) = dirty_key {
        state.dirty_fields.insert(k);
        state.status = format!("{} reset", k);
    }
}

// === Commit =================================================================

/// Write all dirty top-level fields (plus fx_params if dirty) to the XDG
/// config in a single atomic `write_xdg_fields` call. The vgalizer watcher
/// picks up the rename within ~100ms.
pub fn commit_to_disk(state: &mut AppState) {
    if !state.is_dirty() {
        state.status = "nothing to commit".into();
        return;
    }
    let cfg = &state.config;
    let mut updates: Vec<(&str, serde_json::Value)> = Vec::new();

    for key in state.dirty_fields.iter() {
        let v = match *key {
            "dj_name"          => json!(cfg.dj_name),
            "scene_duration"   => json!(cfg.scene_duration),
            "beat_sensitivity" => json!(cfg.beat_sensitivity),
            "strobe_mode"      => json!(cfg.strobe_mode),
            "fx_speed_mult"    => json!(cfg.fx_speed_mult),
            "vga_sync"         => json!(cfg.vga_sync),
            "mirror_count"     => json!(cfg.mirror_count),
            "mirror_spread"    => json!(cfg.mirror_spread),
            "disabled_effects" => match &cfg.disabled_effects {
                Some(list) if !list.is_empty() => json!(list),
                _ => serde_json::Value::Null,
            },
            _ => continue,
        };
        updates.push((*key, v));
    }

    if state.dirty_fx_params {
        updates.push(("fx_params", json!(cfg.fx_params)));
    }

    match config::write_xdg_fields(&updates) {
        Ok(()) => {
            state.mark_committed();
            state.status = format!("committed {} field(s)", updates.len());
        }
        Err(e) => {
            state.status = format!("commit failed: {}", e);
        }
    }
}
