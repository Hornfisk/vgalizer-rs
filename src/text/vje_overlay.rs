//! Unified vje-style param editor overlay (`V`).
//!
//! One-stop deep editor that closes the three gaps versus the standalone
//! `vje` TUI binary:
//!
//! 1. **Cross-effect editing** — browse all 26 effects and nudge the params
//!    of any of them, even while a different effect is rendering. The
//!    single-effect `E` overlay (`params_overlay.rs`) is hard-locked to the
//!    currently active effect; this one is not.
//! 2. **Unified view** — effect list on the left, per-effect params on the
//!    right, matching the layout of `src/bin/vje/ui.rs`.
//! 3. **Viz-shrink preview** — pairs with a proportional blit viewport in
//!    `app.rs` so the running visuals render into a dedicated sub-rect
//!    next to the panel instead of fighting the text underneath.
//!
//! All edits mutate `state.config` directly (same pattern as the existing
//! `G` global-settings overlay). `Enter` flushes every dirty field via a
//! single atomic `config::write_xdg_fields` call; the running visualizer's
//! `ConfigWatcher` then picks the change up within ~100ms and re-uploads
//! params / applies globals exactly as if the edit had come from the
//! standalone `vje` binary over SSH.

use std::collections::HashSet;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use serde_json::json;

use crate::config::Config;
use crate::effects::params::{effect_params, ParamDef};
use crate::effects::EFFECT_NAMES;
use crate::global_settings::GlobalKnob;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VjeTab {
    Effects,
    Globals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VjeEffectsFocus {
    List,
    Params,
}

/// Extra (non-`GlobalKnob`) numeric rows that land in the Globals tab.
/// Kept as a small hand-rolled list so we can mutate the native Config
/// field directly and still persist via `write_xdg_fields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraGlobal {
    SceneDuration,
    BeatSensitivity,
    BeatMaxBpm,
    FxSpeedMult,
}

impl ExtraGlobal {
    pub const ALL: &'static [ExtraGlobal] = &[
        Self::SceneDuration,
        Self::BeatSensitivity,
        Self::BeatMaxBpm,
        Self::FxSpeedMult,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::SceneDuration   => "scene_dur",
            Self::BeatSensitivity => "beat_sens",
            Self::BeatMaxBpm      => "beat_maxbpm",
            Self::FxSpeedMult     => "fx_speed",
        }
    }

    pub fn config_key(&self) -> &'static str {
        match self {
            Self::SceneDuration   => "scene_duration",
            Self::BeatSensitivity => "beat_sensitivity",
            // T6a⁸: bpm_lock_max repurposed as "max detectable BPM",
            // driving the beat tracker's cooldown.
            Self::BeatMaxBpm      => "bpm_lock_max",
            Self::FxSpeedMult     => "fx_speed_mult",
        }
    }

    /// Raw value + its displayable range for the bar graph.
    /// Returns (current, min, max).
    pub fn read(&self, c: &Config) -> (f32, f32, f32) {
        match self {
            Self::SceneDuration   => (c.scene_duration as f32, 3.0, 120.0),
            Self::BeatSensitivity => (c.beat_sensitivity,       0.1, 5.0),
            Self::BeatMaxBpm      => (c.bpm_lock_max,          60.0, 300.0),
            Self::FxSpeedMult     => (c.fx_speed_mult,          0.1, 4.0),
        }
    }

    /// Nudge with a "feels right" step size chosen per knob. `fast` = Shift.
    pub fn nudge(&self, c: &mut Config, dir: i32, fast: bool) {
        let mult = if fast { 10.0 } else { 1.0 };
        let d = dir as f32 * mult;
        match self {
            Self::SceneDuration => {
                c.scene_duration = (c.scene_duration + d as f64 * 1.0).clamp(3.0, 120.0);
            }
            Self::BeatSensitivity => {
                c.beat_sensitivity = (c.beat_sensitivity + d * 0.05).clamp(0.1, 5.0);
            }
            Self::BeatMaxBpm => {
                // 2 BPM per slow tick, 20 per Shift. Fine-grain lets the
                // user walk the cooldown into whatever clips the
                // subdivisions without killing real kicks.
                c.bpm_lock_max = (c.bpm_lock_max + d * 2.0).clamp(60.0, 300.0);
            }
            Self::FxSpeedMult => {
                c.fx_speed_mult = (c.fx_speed_mult + d * 0.05).clamp(0.1, 4.0);
            }
        }
    }

    pub fn to_json(&self, c: &Config) -> serde_json::Value {
        match self {
            Self::SceneDuration   => json!(c.scene_duration),
            Self::BeatSensitivity => json!(c.beat_sensitivity),
            Self::BeatMaxBpm      => json!(c.bpm_lock_max),
            Self::FxSpeedMult     => json!(c.fx_speed_mult),
        }
    }
}

/// Total row count in the Globals tab: 8 post-fx knobs + 3 extras.
fn globals_row_count() -> usize {
    GlobalKnob::ALL.len() + ExtraGlobal::ALL.len()
}

/// In-memory editor state. Owned by `AppState` as `Option<VjeOverlayState>`;
/// `Some` means the overlay is open. Edits mutate `state.config` in place
/// so the next frame shows the new values; `dirty_*` tracks what needs to
/// be written on `Enter`.
pub struct VjeOverlayState {
    pub tab: VjeTab,
    pub effects_focus: VjeEffectsFocus,

    pub effect_cursor: usize, // index into EFFECT_NAMES
    pub param_cursor:  usize, // index into the visible ParamDef list
    pub global_cursor: usize, // 0..globals_row_count()

    /// Top of the effects list window (for scrolling when the list is
    /// taller than the visible area).
    pub effect_list_offset: usize,

    /// Set of "effect.param_name" strings that have been nudged since
    /// open. Used for the `*` dirty marker next to the row.
    pub dirty_params: HashSet<String>,
    pub dirty_globals: HashSet<String>,
    pub dirty_disabled: bool,

    /// Snapshot of `state.config` taken at open-time, used for Esc
    /// cancel/restore behaviour if we ever want it (currently Esc just
    /// closes — uncommitted changes stay in memory but aren't persisted
    /// until the next commit). Kept around for future "revert" feature.
    #[allow(dead_code)]
    pub original_config: Box<Config>,

    pub status: String,

    /// True when the glyphon text buffer needs to be rebuilt this
    /// frame. Set on every mutation (navigation, edit, tab switch,
    /// status update, etc.) and cleared at the end of
    /// `VjeOverlay::update_text`. When the overlay is open but
    /// nothing changed, `update_text` early-returns and we save the
    /// per-frame string-build + glyphon layout work (~0.1 ms).
    pub needs_repaint: bool,
}

impl VjeOverlayState {
    pub fn open(config: &Config) -> Self {
        Self {
            tab: VjeTab::Effects,
            effects_focus: VjeEffectsFocus::List,
            effect_cursor: 0,
            param_cursor: 0,
            global_cursor: 0,
            effect_list_offset: 0,
            dirty_params: HashSet::new(),
            dirty_globals: HashSet::new(),
            dirty_disabled: false,
            original_config: Box::new(config.clone()),
            status: "press ? for help".to_string(),
            // First frame must repaint so the initial layout draws.
            needs_repaint: true,
        }
    }

    /// Set the transient status line and flag the buffer for repaint.
    /// Use this instead of assigning `st.status` directly from outside
    /// the struct so the repaint flag stays consistent.
    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.needs_repaint = true;
    }

    pub fn is_dirty(&self) -> bool {
        !self.dirty_params.is_empty()
            || !self.dirty_globals.is_empty()
            || self.dirty_disabled
    }

    pub fn current_effect(&self) -> &'static str {
        EFFECT_NAMES
            .get(self.effect_cursor)
            .copied()
            .unwrap_or("")
    }

    /// Visible (non-`(unused)`) param definitions for the effect under the
    /// cursor. The cursor operates in visible-index space so arrow keys
    /// skip the placeholder rows.
    pub fn visible_params(&self) -> Vec<&'static ParamDef> {
        effect_params(self.current_effect())
            .iter()
            .filter(|d| d.name != "(unused)")
            .collect()
    }

    pub fn switch_tab(&mut self) {
        self.tab = match self.tab {
            VjeTab::Effects => VjeTab::Globals,
            VjeTab::Globals => VjeTab::Effects,
        };
        self.status.clear();
        self.needs_repaint = true;
    }

    // --- Effects tab navigation --------------------------------------------

    pub fn effect_list_up(&mut self) {
        if EFFECT_NAMES.is_empty() { return; }
        self.effect_cursor = if self.effect_cursor == 0 {
            EFFECT_NAMES.len() - 1
        } else {
            self.effect_cursor - 1
        };
        self.param_cursor = 0;
        self.needs_repaint = true;
    }

    pub fn effect_list_down(&mut self) {
        if EFFECT_NAMES.is_empty() { return; }
        self.effect_cursor = (self.effect_cursor + 1) % EFFECT_NAMES.len();
        self.param_cursor = 0;
        self.needs_repaint = true;
    }

    pub fn focus_params(&mut self) {
        if !self.visible_params().is_empty() {
            self.effects_focus = VjeEffectsFocus::Params;
            self.needs_repaint = true;
        }
    }

    pub fn focus_list(&mut self) {
        self.effects_focus = VjeEffectsFocus::List;
        self.needs_repaint = true;
    }

    pub fn swap_effects_focus(&mut self) {
        match self.effects_focus {
            VjeEffectsFocus::List => self.focus_params(),
            VjeEffectsFocus::Params => self.focus_list(),
        }
    }

    pub fn param_up(&mut self) {
        let n = self.visible_params().len();
        if n == 0 { return; }
        self.param_cursor = if self.param_cursor == 0 { n - 1 } else { self.param_cursor - 1 };
        self.needs_repaint = true;
    }

    pub fn param_down(&mut self) {
        let n = self.visible_params().len();
        if n == 0 { return; }
        self.param_cursor = (self.param_cursor + 1) % n;
        self.needs_repaint = true;
    }

    // --- Effects tab mutations ---------------------------------------------

    /// Nudge the currently-hovered param. Mutates `cfg.fx_params` directly
    /// and marks the row dirty. Does *not* upload to the GPU — the active
    /// effect (if any) re-reads fx_params via the config watcher when the
    /// user hits Enter and the XDG write triggers a reload.
    pub fn nudge_current_param(&mut self, cfg: &mut Config, dir: i32, fast: bool) {
        let effect = self.current_effect().to_string();
        let defs = self.visible_params();
        let Some(def) = defs.get(self.param_cursor).copied() else {
            self.status = format!("{} has no editable params", effect);
            self.needs_repaint = true;
            return;
        };
        let cur = read_param(cfg, &effect, def.name, def.default);
        let mult = if fast { 10.0 } else { 1.0 };
        let next = (cur + dir as f32 * def.step * mult).clamp(def.min, def.max);
        write_param(cfg, &effect, def.name, next);
        self.dirty_params.insert(format!("{}.{}", effect, def.name));
        self.status = format!("{}.{} = {:.3}", effect, def.name, next);
        self.needs_repaint = true;
    }

    pub fn reset_current_param(&mut self, cfg: &mut Config) {
        let effect = self.current_effect().to_string();
        let defs = self.visible_params();
        let Some(def) = defs.get(self.param_cursor).copied() else { return; };
        write_param(cfg, &effect, def.name, def.default);
        self.dirty_params.insert(format!("{}.{}", effect, def.name));
        self.status = format!("{}.{} reset → {:.3}", effect, def.name, def.default);
        self.needs_repaint = true;
    }

    /// Toggle the disabled marker on the effect under the cursor.
    pub fn toggle_disabled(&mut self, cfg: &mut Config) {
        let effect = self.current_effect().to_string();
        if effect.is_empty() { return; }
        let mut list = cfg.disabled_effects.clone().unwrap_or_default();
        if let Some(pos) = list.iter().position(|s| s == &effect) {
            list.remove(pos);
            self.status = format!("enabled {}", effect);
        } else {
            list.push(effect.clone());
            self.status = format!("disabled {}", effect);
        }
        cfg.disabled_effects = if list.is_empty() { None } else { Some(list) };
        self.dirty_disabled = true;
        self.needs_repaint = true;
    }

    // --- Globals tab -------------------------------------------------------

    pub fn global_up(&mut self) {
        let n = globals_row_count();
        if n == 0 { return; }
        self.global_cursor = if self.global_cursor == 0 { n - 1 } else { self.global_cursor - 1 };
        self.needs_repaint = true;
    }

    pub fn global_down(&mut self) {
        let n = globals_row_count();
        if n == 0 { return; }
        self.global_cursor = (self.global_cursor + 1) % n;
        self.needs_repaint = true;
    }

    pub fn nudge_global(&mut self, cfg: &mut Config, dir: i32, fast: bool) {
        let g = self.global_cursor;
        let knob_count = GlobalKnob::ALL.len();
        if g < knob_count {
            let knob = GlobalKnob::ALL[g];
            let mult = if fast { 10.0 } else { 1.0 };
            let cur = knob.read(cfg);
            let nv = (cur + dir as f32 * knob.step() * mult).clamp(0.0, 1.0);
            knob.write(cfg, nv);
            self.dirty_globals.insert(knob.config_key().to_string());
            self.status = format!("{} = {:.2}", knob.label(), nv);
        } else {
            let idx = g - knob_count;
            let Some(extra) = ExtraGlobal::ALL.get(idx).copied() else { return; };
            extra.nudge(cfg, dir, fast);
            self.dirty_globals.insert(extra.config_key().to_string());
            let (cur, _, _) = extra.read(cfg);
            self.status = format!("{} = {:.2}", extra.label(), cur);
        }
        self.needs_repaint = true;
    }

    pub fn reset_global(&mut self, cfg: &mut Config) {
        let g = self.global_cursor;
        let knob_count = GlobalKnob::ALL.len();
        let default = Config::default();
        if g < knob_count {
            let knob = GlobalKnob::ALL[g];
            knob.write(cfg, knob.read(&default));
            self.dirty_globals.insert(knob.config_key().to_string());
            self.status = format!("{} reset", knob.label());
        } else {
            let idx = g - knob_count;
            let Some(extra) = ExtraGlobal::ALL.get(idx).copied() else { return; };
            match extra {
                ExtraGlobal::SceneDuration   => cfg.scene_duration   = default.scene_duration,
                ExtraGlobal::BeatSensitivity => cfg.beat_sensitivity = default.beat_sensitivity,
                ExtraGlobal::BeatMaxBpm      => cfg.bpm_lock_max     = default.bpm_lock_max,
                ExtraGlobal::FxSpeedMult     => cfg.fx_speed_mult    = default.fx_speed_mult,
            }
            self.dirty_globals.insert(extra.config_key().to_string());
            self.status = format!("{} reset", extra.label());
        }
        self.needs_repaint = true;
    }

    // --- Commit ------------------------------------------------------------

    /// Build the `write_xdg_fields` update batch from the current dirty
    /// sets. Returns an empty vec if nothing needs persisting.
    pub fn build_updates(&self, cfg: &Config) -> Vec<(String, serde_json::Value)> {
        let mut updates: Vec<(String, serde_json::Value)> = Vec::new();

        if !self.dirty_params.is_empty() {
            updates.push(("fx_params".to_string(), json!(cfg.fx_params)));
        }

        if self.dirty_disabled {
            let v = match &cfg.disabled_effects {
                Some(list) if !list.is_empty() => json!(list),
                _ => serde_json::Value::Null,
            };
            updates.push(("disabled_effects".to_string(), v));
        }

        for key in &self.dirty_globals {
            // First try a GlobalKnob by matching config_key.
            if let Some(knob) = GlobalKnob::ALL
                .iter()
                .find(|k| k.config_key() == key.as_str())
            {
                updates.push((key.clone(), knob.to_json(cfg)));
                continue;
            }
            // Otherwise an ExtraGlobal.
            if let Some(extra) = ExtraGlobal::ALL
                .iter()
                .find(|e| e.config_key() == key.as_str())
            {
                updates.push((key.clone(), extra.to_json(cfg)));
                continue;
            }
        }

        updates
    }

    pub fn mark_committed(&mut self) {
        self.dirty_params.clear();
        self.dirty_globals.clear();
        self.dirty_disabled = false;
        // The "* MODIFIED" badge in the title bar disappears, so we
        // need to redraw.
        self.needs_repaint = true;
    }
}

// ---------------------------------------------------------------------------
// Shared helpers (mirrors src/bin/vje/edit.rs)
// ---------------------------------------------------------------------------

fn read_param(cfg: &Config, effect: &str, name: &str, default: f32) -> f32 {
    cfg.fx_params
        .get(effect)
        .and_then(|m| m.get(name))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn write_param(cfg: &mut Config, effect: &str, name: &str, value: f32) {
    let m = cfg.fx_params.entry(effect.to_string()).or_default();
    m.insert(name.to_string(), json!(value as f64));
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// How many effect rows to show in the scrolling list at a time. Matches
/// the "11×22 font, scrolling list" operating point from the plan.
const EFFECT_LIST_VISIBLE: usize = 16;

pub struct VjeOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    font_size_px: f32,
    line_height: f32,
}

impl VjeOverlay {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        // Monospace so the vje-style column grid actually lines up.
        // DejaVu Sans Mono is public domain / Bitstream Vera derivative,
        // ~336 KB, shipped on both Debian and Arch by default but bundled
        // here so the binary is self-contained.
        let font_data = include_bytes!("../../assets/fonts/DejaVuSansMono.ttf");
        font_system.db_mut().load_font_data(font_data.to_vec());

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let font_size_px = 20.0;
        let line_height = font_size_px * 1.30;
        let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size_px, line_height));
        // Wide enough that the row strings never wrap; TextBounds clips at
        // render time to the panel rect.
        buffer.set_size(&mut font_system, Some(2000.0), Some(2000.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            font_size_px,
            line_height,
        }
    }

    /// Re-flow the entire panel as a single monospace string. Called every
    /// frame while the overlay is open, but no-ops unless `st.needs_repaint`
    /// is set — panel layout only changes in response to user input, so
    /// on idle frames we keep the previously-set glyphon buffer and skip
    /// the ~0.1 ms string build + layout cost.
    pub fn update_text(&mut self, st: &mut VjeOverlayState, cfg: &Config) {
        if !st.needs_repaint {
            return;
        }

        // Keep the scroll window around the cursor.
        if st.effect_cursor < st.effect_list_offset {
            st.effect_list_offset = st.effect_cursor;
        } else if st.effect_cursor >= st.effect_list_offset + EFFECT_LIST_VISIBLE {
            st.effect_list_offset = st.effect_cursor + 1 - EFFECT_LIST_VISIBLE;
        }

        let mut s = String::new();

        // ---- Title bar ----
        let dirty = if st.is_dirty() { "  * MODIFIED" } else { "" };
        let tab_label = match st.tab {
            VjeTab::Effects => "[Effects]  Globals",
            VjeTab::Globals => " Effects  [Globals]",
        };
        s.push_str(&format!("vje  {}{}\n\n", tab_label, dirty));

        // ---- Body ----
        match st.tab {
            VjeTab::Effects => render_effects_tab(&mut s, st, cfg),
            VjeTab::Globals => render_globals_tab(&mut s, st, cfg),
        }

        // ---- Status / hint ----
        s.push('\n');
        if !st.status.is_empty() {
            s.push_str(&format!("{}\n", st.status));
        }
        s.push_str(hint_line(st));

        self.buffer.set_text(
            &mut self.font_system,
            &s,
            Attrs::new().family(Family::Name("DejaVu Sans Mono")),
            Shaping::Basic,
        );
        self.buffer.shape_until_scroll(&mut self.font_system, false);

        st.needs_repaint = false;
    }

    /// Draw the overlay into `target`. `panel_rect` is the pixel rect on
    /// the swapchain that the panel should occupy — the caller
    /// (`app.rs::render_frame`) computes it proportional to `gpu.size` so
    /// it pairs with the blit viewport used for the viz preview.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        screen_size: (u32, u32),
        panel_rect: (f32, f32, f32, f32),
    ) {
        self.viewport.update(queue, Resolution {
            width: screen_size.0,
            height: screen_size.1,
        });

        let (px, py, pw, ph) = panel_rect;
        let margin = 16.0;

        let areas = [TextArea {
            buffer: &self.buffer,
            left: px + margin,
            top:  py + margin,
            scale: 1.0,
            bounds: TextBounds {
                left:   px.max(0.0) as i32,
                top:    py.max(0.0) as i32,
                right:  (px + pw) as i32,
                bottom: (py + ph) as i32,
            },
            default_color: Color::rgba(230, 230, 230, 255),
            custom_glyphs: &[],
        }];

        self.renderer
            .prepare(
                device, queue, &mut self.font_system, &mut self.atlas,
                &self.viewport, areas, &mut self.swash_cache,
            )
            .expect("vje overlay prepare failed");

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vje_overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .expect("vje overlay render failed");
        }

        self.atlas.trim();
    }

    #[allow(dead_code)]
    pub fn font_px(&self) -> f32 { self.font_size_px }
    #[allow(dead_code)]
    pub fn line_height(&self) -> f32 { self.line_height }
}

// ---------------------------------------------------------------------------
// Body formatting
// ---------------------------------------------------------------------------

const BAR_W: usize = 14;

fn render_effects_tab(s: &mut String, st: &VjeOverlayState, cfg: &Config) {
    // Column widths chosen so the full row fits in ~60 monospace chars,
    // which at a 20 px font hits the ~660 px panel width from the plan.
    //
    // Layout (per body row):
    //   <20 char effect list col>  <params col>
    //
    // The effect list column is padded out to a fixed width so each body
    // row is a single "left_cell + right_cell" concatenation.
    const LIST_COL_W: usize = 20;

    let disabled = cfg.disabled_effects.clone().unwrap_or_default();
    let visible_params: Vec<&'static ParamDef> = effect_params(st.current_effect())
        .iter()
        .filter(|d| d.name != "(unused)")
        .collect();

    // Body row count: whichever side is taller drives the total number of
    // lines. The list side is capped at EFFECT_LIST_VISIBLE; the params
    // side is uncapped but realistically never exceeds 8.
    let list_rows  = EFFECT_LIST_VISIBLE.min(EFFECT_NAMES.len());
    let param_rows = visible_params.len().max(1); // always at least 1 ("no editable params")
    let body_rows  = list_rows.max(param_rows + 2); // +2 for the "params: name" header + blank

    for row in 0..body_rows {
        // ---- Left cell: effect list row ----
        let list_cell = {
            let list_row = row; // rows[0..list_rows] map onto the scrollable window
            if list_row < list_rows {
                let i = st.effect_list_offset + list_row;
                if i < EFFECT_NAMES.len() {
                    let name = EFFECT_NAMES[i];
                    let is_disabled = disabled.iter().any(|d| d == name);
                    let is_cursor = i == st.effect_cursor;
                    let focused_here = matches!(st.effects_focus, VjeEffectsFocus::List);
                    let marker = if is_cursor && focused_here { ">" }
                                 else if is_cursor { "·" }
                                 else { " " };
                    let dmark = if is_disabled { "x" } else { " " };
                    // Trim long names so the column stays aligned.
                    let shown = if name.len() > LIST_COL_W - 4 {
                        &name[..LIST_COL_W - 4]
                    } else {
                        name
                    };
                    format!("{} {} {:<width$}", marker, dmark, shown, width = LIST_COL_W - 4)
                } else {
                    " ".repeat(LIST_COL_W)
                }
            } else {
                " ".repeat(LIST_COL_W)
            }
        };

        // ---- Right cell: params table row ----
        let right_cell = {
            // Row 0 is the "params: <effect>" header; row 1 is blank; body
            // rows start at row 2.
            if row == 0 {
                format!("params: {}", st.current_effect())
            } else if row == 1 {
                String::new()
            } else {
                let pr = row - 2;
                if visible_params.is_empty() {
                    if pr == 0 {
                        "(no editable params)".to_string()
                    } else {
                        String::new()
                    }
                } else if let Some(def) = visible_params.get(pr) {
                    let cur = read_param(cfg, st.current_effect(), def.name, def.default);
                    let bar = value_bar(cur, def.min, def.max, BAR_W);
                    let is_cursor = pr == st.param_cursor;
                    let focused_here = matches!(st.effects_focus, VjeEffectsFocus::Params);
                    let marker = if is_cursor && focused_here { ">" }
                                 else if is_cursor { "·" }
                                 else { " " };
                    let dkey = format!("{}.{}", st.current_effect(), def.name);
                    let dirty = if st.dirty_params.contains(&dkey) { "*" } else { " " };
                    format!("{} {} {:<11} {} {:>5.2}", marker, dirty, def.name, bar, cur)
                } else {
                    String::new()
                }
            }
        };

        s.push_str(&list_cell);
        s.push_str("  ");
        s.push_str(&right_cell);
        s.push('\n');
    }

    // Scroll indicator under the list if there are more entries below.
    if st.effect_list_offset + EFFECT_LIST_VISIBLE < EFFECT_NAMES.len() {
        s.push_str(&format!(
            "{:<width$}  (more below…)\n",
            "",
            width = LIST_COL_W
        ));
    }
}

fn render_globals_tab(s: &mut String, st: &VjeOverlayState, cfg: &Config) {
    let knob_count = GlobalKnob::ALL.len();

    for (i, knob) in GlobalKnob::ALL.iter().enumerate() {
        let cur = knob.read(cfg);
        let is_cursor = i == st.global_cursor;
        let marker = if is_cursor { ">" } else { " " };
        let bar = value_bar(cur, 0.0, 1.0, BAR_W);
        let dirty = if st.dirty_globals.contains(knob.config_key()) { "*" } else { " " };
        s.push_str(&format!(
            "{} {} {:<11} {} {:>5.2}\n",
            marker, dirty, knob.label(), bar, cur
        ));
    }

    s.push('\n');

    for (j, extra) in ExtraGlobal::ALL.iter().enumerate() {
        let i = knob_count + j;
        let (cur, min, max) = extra.read(cfg);
        let is_cursor = i == st.global_cursor;
        let marker = if is_cursor { ">" } else { " " };
        let bar = value_bar(cur, min, max, BAR_W);
        let dirty = if st.dirty_globals.contains(extra.config_key()) { "*" } else { " " };
        s.push_str(&format!(
            "{} {} {:<11} {} {:>5.2}\n",
            marker, dirty, extra.label(), bar, cur
        ));
    }
}

fn hint_line(st: &VjeOverlayState) -> &'static str {
    match st.tab {
        VjeTab::Effects => match st.effects_focus {
            VjeEffectsFocus::List => {
                "↑↓ move  Space→params  X disable  Tab globals  Enter save  V/Esc close"
            }
            VjeEffectsFocus::Params => {
                "↑↓ row  ←→ nudge (Shift ×10)  R reset  Space→list  Enter save  Esc close"
            }
        },
        VjeTab::Globals => {
            "↑↓ row  ←→ nudge (Shift ×10)  R reset  Tab effects  Enter save  V/Esc close"
        }
    }
}

fn value_bar(value: f32, min: f32, max: f32, width: usize) -> String {
    if max <= min || width == 0 {
        return String::new();
    }
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let filled = (t * width as f32).round() as usize;
    let mut out = String::with_capacity(width + 2);
    out.push('[');
    for i in 0..width {
        out.push(if i < filled { '#' } else { '-' });
    }
    out.push(']');
    out
}
