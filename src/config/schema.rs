use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dj_name: String,
    pub resolution: Option<(u32, u32)>,
    pub fullscreen: bool,
    pub audio_device: Option<String>,
    pub scene_duration: f64,
    pub beat_sensitivity: f32,
    pub strobe_mode: String,
    pub trail_alpha: u32,
    pub global_vibration: f32,
    pub global_rotation: f32,
    pub glitch_intensity: f32,
    pub fx_speed_mult: f32,
    pub vga_intensity: f32,
    pub vga_ca: u32,
    pub vga_noise: f32,
    pub vga_sync: f32,
    pub mirror_pool: Vec<String>,
    /// Seconds between automatic mirror-mode rotations within a single
    /// scene. Multiplies visual variety: with a 30s scene and a 6s mirror
    /// interval you get ~5 different mirror framings per effect. `0`
    /// disables auto-cycling (only scene switches and manual P change it).
    pub mirror_cycle_interval: f64,
    pub mirror_alpha: u32,
    pub mirror_count: u32,
    pub mirror_spread: i32,
    /// Names of effects the user has explicitly turned off in the M menu.
    /// Stored as a *deny list* so any newly added effect appears enabled by
    /// default after a code update — the user only has to remember what they
    /// turned off, not re-enable everything new. `None` or empty = all on.
    #[serde(default)]
    pub disabled_effects: Option<Vec<String>>,
    pub fx_params: HashMap<String, HashMap<String, serde_json::Value>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dj_name: "DJ NAME".to_string(),
            resolution: None,
            fullscreen: true,
            audio_device: None,
            scene_duration: 30.0,
            beat_sensitivity: 1.4,
            strobe_mode: "beat".to_string(),
            trail_alpha: 40,
            global_vibration: 0.5,
            global_rotation: 0.3,
            glitch_intensity: 0.4,
            fx_speed_mult: 1.0,
            vga_intensity: 0.3,
            vga_ca: 4,
            vga_noise: 0.15,
            vga_sync: 0.08,
            mirror_pool: vec![
                "none".to_string(),
                "none".to_string(),
                "mirror_h".to_string(),
                "mirror_v".to_string(),
                "mirror_quad".to_string(),
                "kaleido".to_string(),
            ],
            mirror_cycle_interval: 6.0,
            mirror_alpha: 160,
            mirror_count: 6,
            mirror_spread: 8,
            disabled_effects: None,
            fx_params: HashMap::new(),
        }
    }
}
