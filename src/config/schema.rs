use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dj_name: String,
    pub resolution: Option<(u32, u32)>,
    pub fullscreen: bool,
    pub target_fps: u32,
    pub audio_device: Option<String>,
    pub scene_duration: f64,
    pub beat_sensitivity: f32,
    pub strobe_mode: String,
    pub trail_alpha: u32,
    pub name_font_size: f32,
    pub global_vibration: f32,
    pub global_vib_division: String,
    pub global_rotation: f32,
    pub glitch_intensity: f32,
    pub fx_speed_mult: f32,
    pub vga_intensity: f32,
    pub vga_ca: u32,
    pub vga_noise: f32,
    pub vga_sync: f32,
    pub spectrum_n_bands: u32,
    pub spectrum_height: f32,
    pub spectrum_glow: f32,
    pub spectrum_color: String,
    pub spectrum_anchor: String,
    pub mirror_pool: Vec<String>,
    pub mirror_alpha: u32,
    pub mirror_count: u32,
    pub mirror_spread: i32,
    pub kaleido_post_alpha: u32,
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
            target_fps: 60,
            audio_device: None,
            scene_duration: 30.0,
            beat_sensitivity: 1.4,
            strobe_mode: "beat".to_string(),
            trail_alpha: 40,
            name_font_size: 0.38,
            global_vibration: 0.5,
            global_vib_division: "beat".to_string(),
            global_rotation: 0.3,
            glitch_intensity: 0.4,
            fx_speed_mult: 1.0,
            vga_intensity: 0.3,
            vga_ca: 4,
            vga_noise: 0.15,
            vga_sync: 0.08,
            spectrum_n_bands: 32,
            spectrum_height: 0.6,
            spectrum_glow: 0.4,
            spectrum_color: "palette".to_string(),
            spectrum_anchor: "bottom".to_string(),
            mirror_pool: vec![
                "none".to_string(),
                "none".to_string(),
                "mirror_h".to_string(),
                "mirror_v".to_string(),
                "mirror_quad".to_string(),
                "kaleido".to_string(),
            ],
            mirror_alpha: 160,
            mirror_count: 6,
            mirror_spread: 8,
            kaleido_post_alpha: 140,
            disabled_effects: None,
            fx_params: HashMap::new(),
        }
    }
}
