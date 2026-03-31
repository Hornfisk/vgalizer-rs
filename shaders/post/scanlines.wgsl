// Scanlines: CRT-style horizontal line dimming.

struct GlobalUniforms {
    time: f32, dt: f32, beat_time: f32, fx_speed: f32,
    resolution: vec2<f32>, _pad1: vec2<f32>,
    level: f32, pulse: f32, beat: f32, half_beat: f32,
    quarter_beat: f32, bpm: f32, _pad2: vec2<f32>,
    bands: array<vec4<f32>, 8>,
    palette_sa: vec4<f32>, palette_sb: vec4<f32>,
    palette_ra: vec4<f32>, palette_rb: vec4<f32>,
};

struct PostUniforms {
    trail_alpha: f32, glitch_intensity: f32, vga_intensity: f32, vga_ca: f32,
    vga_noise: f32, vga_sync: f32, rotation_angle: f32, vibration_y: f32,
    strobe_alpha: f32, strobe_r: f32, strobe_g: f32, strobe_b: f32,
    mirror_mode: u32, mirror_alpha: f32, mirror_count: u32, mirror_spread: f32,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> post: PostUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, tex_sampler, uv);
    // Darken every 3rd pixel row
    let scanline = step(0.5, fract(uv.y * globals.resolution.y / 3.0));
    let dim = mix(0.78, 1.0, scanline);
    return vec4<f32>(color.rgb * dim, 1.0);
}
