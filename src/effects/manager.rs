use rand::Rng;
use std::time::Instant;

use crate::audio::BeatState;
use crate::colors::{palette, palette_count};

use super::EFFECT_NAMES;

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
}

pub struct SceneManager {
    effect_names: Vec<String>,
    current_index: usize,
    palette_index: usize,
    mirror_pool: Vec<MirrorMode>,
    current_mirror: MirrorMode,
    last_switch: Instant,
    scene_duration: f64,
    rng: rand::rngs::ThreadRng,
}

impl SceneManager {
    pub fn new(effect_names: Vec<String>, mirror_pool_strs: &[String], scene_duration: f64) -> Self {
        let mirror_pool: Vec<MirrorMode> = mirror_pool_strs.iter().map(|s| MirrorMode::from_str(s)).collect();
        let mut rng = rand::thread_rng();
        let current_mirror = mirror_pool[rng.gen_range(0..mirror_pool.len())];
        Self {
            effect_names,
            current_index: 0,
            palette_index: 0,
            mirror_pool,
            current_mirror,
            last_switch: Instant::now(),
            scene_duration,
            rng,
        }
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
    pub fn update(&mut self, _beat: &BeatState) -> bool {
        if self.last_switch.elapsed().as_secs_f64() >= self.scene_duration {
            self.advance();
            return true;
        }
        false
    }

    /// Advance to the next effect (SPACE key or auto-switch).
    pub fn advance(&mut self) {
        self.current_index = (self.current_index + 1) % self.effect_names.len();
        self.palette_index = (self.palette_index + 1) % palette_count();
        self.current_mirror = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
        self.last_switch = Instant::now();
        log::info!(
            "Scene: {} | palette: {} | mirror: {:?}",
            self.current_effect(),
            self.palette_index,
            self.current_mirror
        );
    }

    /// Jump to a specific effect by 1-based index (keys 1-9).
    pub fn jump_to(&mut self, idx: usize) {
        let i = idx.saturating_sub(1).min(self.effect_names.len() - 1);
        self.current_index = i;
        self.palette_index = (self.palette_index + 1) % palette_count();
        self.current_mirror = self.mirror_pool[self.rng.gen_range(0..self.mirror_pool.len())];
        self.last_switch = Instant::now();
    }

    pub fn cycle_mirror(&mut self) {
        let idx = self.mirror_pool.iter().position(|&m| m == self.current_mirror).unwrap_or(0);
        self.current_mirror = self.mirror_pool[(idx + 1) % self.mirror_pool.len()];
    }

    pub fn scene_progress(&self) -> f32 {
        (self.last_switch.elapsed().as_secs_f64() / self.scene_duration) as f32
    }
}
