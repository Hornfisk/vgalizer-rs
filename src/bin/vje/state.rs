//! App state for vje (vj edit) TUI.
//!
//! Holds the working `Config`, an `original` snapshot for revert, and a
//! dirty set of top-level keys to diff at commit time. The visualizer's
//! notify watcher picks up any XDG write, so commit = write + done.

use std::collections::HashSet;
use vgalizer::config::Config;
use vgalizer::effects::params::{effect_params, ParamDef};
use vgalizer::effects::EFFECT_NAMES;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Effects,
    Globals,
}

/// Which pane has keyboard focus inside the Effects tab.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EffectsFocus {
    List,   // moving through EFFECT_NAMES
    Params, // editing params of the selected effect
}

pub struct AppState {
    pub tab: Tab,
    pub effects_focus: EffectsFocus,

    pub config: Config,
    pub original: Config,

    pub effect_cursor: usize,
    pub param_cursor: usize,
    pub global_cursor: usize,

    /// Top-level XDG fields that have been edited this session.
    /// On commit we diff these against `original` and emit a single
    /// atomic `write_xdg_fields` call.
    pub dirty_fields: HashSet<&'static str>,
    pub dirty_fx_params: bool,

    pub help_open: bool,
    pub quit: bool,
    pub status: String,

    /// When Some, we're in text-input mode for dj_name; buffer holds
    /// the edited string and Enter commits it into `config.dj_name`.
    pub dj_name_edit: Option<String>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            tab: Tab::Effects,
            effects_focus: EffectsFocus::List,
            original: config.clone(),
            config,
            effect_cursor: 0,
            param_cursor: 0,
            global_cursor: 0,
            dirty_fields: HashSet::new(),
            dirty_fx_params: false,
            help_open: false,
            quit: false,
            status: String::from("vje — ? for help"),
            dj_name_edit: None,
        }
    }

    pub fn current_effect(&self) -> &'static str {
        EFFECT_NAMES[self.effect_cursor.min(EFFECT_NAMES.len() - 1)]
    }

    pub fn current_effect_params(&self) -> &'static [ParamDef] {
        effect_params(self.current_effect())
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty_fields.is_empty() || self.dirty_fx_params
    }

    pub fn revert(&mut self) {
        self.config = self.original.clone();
        self.dirty_fields.clear();
        self.dirty_fx_params = false;
        self.dj_name_edit = None;
        self.status = String::from("reverted");
    }

    pub fn mark_committed(&mut self) {
        self.original = self.config.clone();
        self.dirty_fields.clear();
        self.dirty_fx_params = false;
    }
}

// === Globals tab row descriptors ============================================

/// One row on the Globals tab. Drives both rendering (label + current value)
/// and editing (nudge dispatch). Read-only rows set `editable = false`.
#[derive(Clone, Copy)]
pub struct GlobalRow {
    pub label: &'static str,
    pub kind: GlobalKind,
    pub editable: bool,
}

#[derive(Clone, Copy)]
pub enum GlobalKind {
    // === Scene ===
    SceneDuration,   // f64 seconds, 1.0 step
    BeatSensitivity, // f32, 0.05 step
    StrobeMode,      // cycle: off -> beat -> half -> quarter
    FxSpeedMult,     // f32, 0.05 step

    // === VGA / post ===
    VgaSync, // f32, 0.01 step (not in G menu)

    // === Mirror ===
    MirrorCount,  // u32, 1 step
    MirrorSpread, // i32, 1 step

    // === Text ===
    DjName, // string, text-input mode

    // === Read-only display ===
    AudioDevice,     // current device (change via in-app A)
    Fullscreen,      // bool
    Resolution,      // Option<(u32,u32)>
    TrailAlpha,      // already in G menu
    VgaCa,           // already in G menu
    VgaIntensity,    // already in G menu
    VgaNoise,        // already in G menu
    GlitchIntensity, // already in G menu
    MirrorAlpha,     // already in G menu
    GlobalRotation,  // already in G menu
    GlobalVibration, // already in G menu
}

pub const GLOBAL_ROWS: &[GlobalRow] = &[
    // Editable
    GlobalRow { label: "dj_name",         kind: GlobalKind::DjName,          editable: true },
    GlobalRow { label: "scene_duration",  kind: GlobalKind::SceneDuration,   editable: true },
    GlobalRow { label: "beat_sensitivity",kind: GlobalKind::BeatSensitivity, editable: true },
    GlobalRow { label: "strobe_mode",     kind: GlobalKind::StrobeMode,      editable: true },
    GlobalRow { label: "fx_speed_mult",   kind: GlobalKind::FxSpeedMult,     editable: true },
    GlobalRow { label: "vga_sync",        kind: GlobalKind::VgaSync,         editable: true },
    GlobalRow { label: "mirror_count",    kind: GlobalKind::MirrorCount,     editable: true },
    GlobalRow { label: "mirror_spread",   kind: GlobalKind::MirrorSpread,    editable: true },
    // Read-only (info)
    GlobalRow { label: "audio_device",    kind: GlobalKind::AudioDevice,     editable: false },
    GlobalRow { label: "fullscreen",      kind: GlobalKind::Fullscreen,      editable: false },
    GlobalRow { label: "resolution",      kind: GlobalKind::Resolution,      editable: false },
    GlobalRow { label: "— G-menu knobs (read-only) —", kind: GlobalKind::TrailAlpha, editable: false },
    GlobalRow { label: "trail_alpha",     kind: GlobalKind::TrailAlpha,      editable: false },
    GlobalRow { label: "vga_ca",          kind: GlobalKind::VgaCa,           editable: false },
    GlobalRow { label: "vga_intensity",   kind: GlobalKind::VgaIntensity,    editable: false },
    GlobalRow { label: "vga_noise",       kind: GlobalKind::VgaNoise,        editable: false },
    GlobalRow { label: "glitch_intensity",kind: GlobalKind::GlitchIntensity, editable: false },
    GlobalRow { label: "mirror_alpha",    kind: GlobalKind::MirrorAlpha,     editable: false },
    GlobalRow { label: "global_rotation", kind: GlobalKind::GlobalRotation,  editable: false },
    GlobalRow { label: "global_vibration",kind: GlobalKind::GlobalVibration, editable: false },
];

pub fn global_value_string(cfg: &Config, kind: GlobalKind) -> String {
    match kind {
        GlobalKind::DjName          => cfg.dj_name.clone(),
        GlobalKind::SceneDuration   => format!("{:.1}", cfg.scene_duration),
        GlobalKind::BeatSensitivity => format!("{:.2}", cfg.beat_sensitivity),
        GlobalKind::StrobeMode      => cfg.strobe_mode.clone(),
        GlobalKind::FxSpeedMult     => format!("{:.2}", cfg.fx_speed_mult),
        GlobalKind::VgaSync         => format!("{:.3}", cfg.vga_sync),
        GlobalKind::MirrorCount     => cfg.mirror_count.to_string(),
        GlobalKind::MirrorSpread    => cfg.mirror_spread.to_string(),
        GlobalKind::AudioDevice     => cfg.audio_device.clone().unwrap_or_else(|| "(default)".into()),
        GlobalKind::Fullscreen      => cfg.fullscreen.to_string(),
        GlobalKind::Resolution      => cfg
            .resolution
            .map(|(w, h)| format!("{}x{}", w, h))
            .unwrap_or_else(|| "(auto)".into()),
        GlobalKind::TrailAlpha      => cfg.trail_alpha.to_string(),
        GlobalKind::VgaCa           => cfg.vga_ca.to_string(),
        GlobalKind::VgaIntensity    => format!("{:.2}", cfg.vga_intensity),
        GlobalKind::VgaNoise        => format!("{:.2}", cfg.vga_noise),
        GlobalKind::GlitchIntensity => format!("{:.2}", cfg.glitch_intensity),
        GlobalKind::MirrorAlpha     => cfg.mirror_alpha.to_string(),
        GlobalKind::GlobalRotation  => format!("{:.2}", cfg.global_rotation),
        GlobalKind::GlobalVibration => format!("{:.2}", cfg.global_vibration),
    }
}
