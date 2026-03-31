use bytemuck::{Pod, Zeroable};

/// Shared uniform buffer passed to ALL shaders via @group(0) @binding(0).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GlobalUniforms {
    pub time: f32,
    pub dt: f32,
    pub beat_time: f32,  // seconds since last beat
    pub fx_speed: f32,

    pub resolution: [f32; 2],
    pub _pad1: [f32; 2],

    pub level: f32,
    pub pulse: f32,      // smoothed beat pulse 0-1, decays between beats
    pub beat: f32,       // 1.0 on beat frame, else 0.0
    pub half_beat: f32,

    pub quarter_beat: f32,
    pub bpm: f32,
    pub _pad2: [f32; 2],

    /// 32 bands packed as 8 vec4s
    pub bands: [[f32; 4]; 8],

    pub palette_sa: [f32; 4],
    pub palette_sb: [f32; 4],
    pub palette_ra: [f32; 4],
    pub palette_rb: [f32; 4],
}

/// Per-effect parameters, @group(1) @binding(0).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct EffectUniforms {
    pub params: [f32; 16],
    pub seed: f32,
    pub _pad: [f32; 3],
}

/// Post-processing parameters, @group(1) @binding(2).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct PostUniforms {
    pub trail_alpha: f32,
    pub glitch_intensity: f32,
    pub vga_intensity: f32,
    pub vga_ca: f32,

    pub vga_noise: f32,
    pub vga_sync: f32,
    pub rotation_angle: f32,
    pub vibration_y: f32,

    pub strobe_alpha: f32,
    pub strobe_r: f32,
    pub strobe_g: f32,
    pub strobe_b: f32,

    pub mirror_mode: u32,   // 0=none 1=h 2=v 3=quad 4=kaleido
    pub mirror_alpha: f32,
    pub mirror_count: u32,
    pub mirror_spread: f32,
}

pub fn pack_bands(bands: &[f32; 32]) -> [[f32; 4]; 8] {
    let mut out = [[0.0f32; 4]; 8];
    for (i, chunk) in bands.chunks(4).enumerate() {
        out[i] = [chunk[0], chunk[1], chunk[2], chunk[3]];
    }
    out
}
