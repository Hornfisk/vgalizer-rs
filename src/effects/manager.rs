use rand::Rng;
use std::time::Instant;

use crate::audio::BeatState;
use crate::colors::palette_count;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MirrorMode {
    None,
    H,
    V,
    Quad,
    Kaleido,
}

impl MirrorMode {
    pub fn as_u32(self) -> u32 {
        match self {
            MirrorMode::None => 0,
            MirrorMode::H => 1,
            MirrorMode::V => 2,
            MirrorMode::Quad => 3,
            MirrorMode::Kaleido => 4,
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "mirror_h" => MirrorMode::H,
            "mirror_v" => MirrorMode::V,
            "mirror_quad" => MirrorMode::Quad,
            "kaleido" => MirrorMode::Kaleido,
            _ => MirrorMode::None,
        }
    }
    /// Deterministic cycle order for the P key. Walks all five variants and
    /// wraps around — independent of the mirror_pool, which can contain
    /// duplicates for weighting the autopilot's random draws.
    pub fn next(self) -> Self {
        match self {
            MirrorMode::None => MirrorMode::H,
            MirrorMode::H => MirrorMode::V,
            MirrorMode::V => MirrorMode::Quad,
            MirrorMode::Quad => MirrorMode::Kaleido,
            MirrorMode::Kaleido => MirrorMode::None,
        }
    }
}

pub struct SceneManager {
    effect_names: Vec<String>,
    /// Parallel to `effect_names`. `false` skips the effect on autopilot
    /// cycling and on `advance()`. The current effect is always allowed.
    enabled: Vec<bool>,
    current_index: usize,
    palette_index: usize,
    mirror_pool: Vec<MirrorMode>,
    current_mirror: MirrorMode,
    last_switch: Instant,
    scene_duration: f64,
    /// Independent mirror auto-cycle timer. Rotates the mirror mode
    /// mid-scene to multiply visual variety. <= 0 disables auto-cycling.
    mirror_cycle_interval: f64,
    last_mirror_cycle: Instant,
    rng: rand::rngs::ThreadRng,
}

impl SceneManager {
    pub fn new(
        effect_names: Vec<String>,
        mirror_pool_strs: &[String],
        scene_duration: f64,
        mirror_cycle_interval: f64,
        disabled_filter: Option<&[String]>,
    ) -> Self {
        let mirror_pool: Vec<MirrorMode> = mirror_pool_strs.iter().map(|s| MirrorMode::from_str(s)).collect();
        let mut rng = rand::thread_rng();
        let current_mirror = mirror_pool[rng.gen_range(0..mirror_pool.len())];
        // Deny-list semantics: anything not in the list is enabled, so newly
        // added effects show up automatically after a code update.
        let enabled: Vec<bool> = match disabled_filter {
            Some(list) => effect_names
                .iter()
                .map(|n| !list.iter().any(|e| e == n))
                .collect(),
            None => vec![true; effect_names.len()],
        };
        // Pick a starting index that is enabled, if any.
        let current_index = enabled.iter().position(|&b| b).unwrap_or(0);
        let now = Instant::now();
        Self {
            effect_names,
            enabled,
            current_index,
            palette_index: 0,
            mirror_pool,
            current_mirror,
            last_switch: now,
            scene_duration,
            mirror_cycle_interval,
            last_mirror_cycle: now,
            rng,
        }
    }

    pub fn effect_names(&self) -> &[String] {
        &self.effect_names
    }

    pub fn enabled(&self) -> &[bool] {
        &self.enabled
    }

    /// Replace the enabled mask using a *deny list*. Names not present in
    /// the registry are ignored. Pass `None` (or empty) to enable
    /// everything.
    pub fn set_disabled_filter(&mut self, disabled: Option<&[String]>) {
        self.enabled = match disabled {
            Some(list) => self
                .effect_names
                .iter()
                .map(|n| !list.iter().any(|e| e == n))
                .collect(),
            None => vec![true; self.effect_names.len()],
        };
        // If the current effect was just disabled, jump to the next enabled
        // one so autopilot doesn't get stuck on a hidden effect.
        if !self.enabled.iter().any(|&b| b) {
            // Avoid an empty selection — fall back to all-enabled.
            self.enabled = vec![true; self.effect_names.len()];
        }
        if !self.enabled[self.current_index] {
            self.advance();
        }
    }

    /// Toggle a single effect by name. Returns the new enabled state, or
    /// `None` if the name is not present.
    pub fn toggle_effect(&mut self, name: &str) -> Option<bool> {
        let i = self.effect_names.iter().position(|n| n == name)?;
        self.enabled[i] = !self.enabled[i];
        // Don't allow zero enabled — re-enable the one we just toggled off.
        if !self.enabled.iter().any(|&b| b) {
            self.enabled[i] = true;
        }
        Some(self.enabled[i])
    }

    pub fn current_effect(&self) -> &str {
        &self.effect_names[self.current_index]
    }

    pub fn current_mirror(&self) -> MirrorMode {
        self.current_mirror
    }

    pub fn current_palette_index(&self) -> usize {
        self.palette_index
    }

    /// Called every frame. Returns true if the scene switched.
    ///
    /// Also drives the independent mirror auto-cycle timer: when the mirror
    /// interval elapses without a scene switch, pick a fresh mirror from the
    /// pool to multiply visual variety mid-scene. Scene switches already
    /// randomize the mirror in `advance()`, so the mirror timer is reset
    /// there too to avoid double-swaps in quick succession.
    pub fn update(&mut self, _beat: &BeatState) -> bool {
        if self.last_switch.elapsed().as_secs_f64() >= self.scene_duration {
            self.advance();
            return true;
        }
        if self.mirror_cycle_interval > 0.0
            && self.last_mirror_cycle.elapsed().as_secs_f64() >= self.mirror_cycle_interval
        {
            self.random_mirror_from_pool();
            self.last_mirror_cycle = Instant::now();
        }
        false
    }

    /// Draw a fresh mirror from the pool, trying to avoid picking the same
    /// mode as the current one when the pool has more than one distinct
    /// variant.
    fn random_mirror_from_pool(&mut self) {
        if self.mirror_pool.is_empty() {
            return;
        }
        for _ in 0..6 {
            let pick = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
            if pick != self.current_mirror {
                self.current_mirror = pick;
                return;
            }
        }
        // Pool is effectively single-mode — accept the redraw.
        self.current_mirror = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
    }

    /// Advance to the next *enabled* effect (SPACE key or auto-switch).
    pub fn advance(&mut self) {
        let n = self.effect_names.len();
        // Walk forward up to n steps to find the next enabled effect.
        let mut i = self.current_index;
        for _ in 0..n {
            i = (i + 1) % n;
            if self.enabled[i] {
                break;
            }
        }
        self.current_index = i;
        self.palette_index = (self.palette_index + 1) % palette_count();
        self.current_mirror = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
        let now = Instant::now();
        self.last_switch = now;
        self.last_mirror_cycle = now;
        log::info!(
            "Scene: {} | palette: {} | mirror: {:?}",
            self.current_effect(),
            self.palette_index,
            self.current_mirror
        );
    }

    /// Jump to a specific effect by 1-based index (keys 1-9). Ignores the
    /// enabled filter — manual selection always wins.
    pub fn jump_to(&mut self, idx: usize) {
        let i = idx.saturating_sub(1).min(self.effect_names.len() - 1);
        self.current_index = i;
        self.palette_index = (self.palette_index + 1) % palette_count();
        self.current_mirror = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
        let now = Instant::now();
        self.last_switch = now;
        self.last_mirror_cycle = now;
    }

    /// Manual P-key cycle. Walks the full MirrorMode enum with wraparound,
    /// independent of `mirror_pool` — otherwise duplicates in the pool
    /// (e.g. the default `[none, none, ...]`) trap cycling on None, since
    /// `position()` always returns the first match.
    pub fn cycle_mirror(&mut self) {
        self.current_mirror = self.current_mirror.next();
        // User just expressed intent — give their pick a full interval to
        // breathe before the auto-cycle considers swapping it.
        self.last_mirror_cycle = Instant::now();
    }

    pub fn scene_progress(&self) -> f32 {
        (self.last_switch.elapsed().as_secs_f64() / self.scene_duration) as f32
    }

    pub fn scene_duration(&self) -> f64 {
        self.scene_duration
    }

    /// Update the autopilot scene duration in seconds. Clamped to a sane
    /// range (3..=300s). Does not reset the current scene timer, so the
    /// new duration takes effect on the next switch.
    pub fn set_scene_duration(&mut self, secs: f64) {
        self.scene_duration = secs.clamp(3.0, 300.0);
    }

    /// Update the mirror auto-cycle interval in seconds. `0.0` (or any
    /// value below the min) disables auto-cycling; otherwise clamped to
    /// 1..=120s. Does not reset the mirror timer.
    pub fn set_mirror_cycle_interval(&mut self, secs: f64) {
        self.mirror_cycle_interval = if secs <= 0.0 { 0.0 } else { secs.clamp(1.0, 120.0) };
    }

    pub fn mirror_cycle_interval(&self) -> f64 {
        self.mirror_cycle_interval
    }

    /// Replace the mirror pool used by autopilot scene switches. The current
    /// mirror keeps showing until the next switch, so changing the pool live
    /// (e.g. via vje) doesn't yank what's on screen.
    pub fn set_mirror_pool(&mut self, mirror_pool_strs: &[String]) {
        let pool: Vec<MirrorMode> = mirror_pool_strs
            .iter()
            .map(|s| MirrorMode::from_str(s))
            .collect();
        if !pool.is_empty() {
            self.mirror_pool = pool;
        }
    }
}
