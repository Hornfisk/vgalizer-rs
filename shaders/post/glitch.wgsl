// Glitch: slice displacement + chromatic ghost + block corruption.

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

fn hash2(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(127.1, 311.7));
    q += dot(q, q + 19.19);
    return fract(q.x * q.y);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let intensity = post.glitch_intensity;
    if intensity <= 0.0 { return textureSample(input_texture, tex_sampler, uv); }

    var suv = uv;
    let t = globals.time;

    // 1. Horizontal slice displacement
    let slice_row = floor(uv.y * 30.0);
    let noise = hash2(vec2<f32>(slice_row, floor(t * 12.0)));
    if noise < intensity * 0.4 {
        let displacement = (noise - 0.5) * intensity * 0.15;
        suv.x = fract(suv.x + displacement);
    }

    // 2. Block corruption: copy from random source block
    let block_x = floor(uv.x * 8.0);
    let block_y = floor(uv.y * 6.0);
    let block_noise = hash2(vec2<f32>(block_x + floor(t * 5.0), block_y));
    if block_noise < intensity * 0.15 {
        let src_block_x = hash2(vec2<f32>(block_noise, 0.1));
        let src_block_y = hash2(vec2<f32>(block_noise, 0.2));
        suv = vec2<f32>(
            (block_x + src_block_x) / 8.0,
            (block_y + src_block_y) / 6.0
        );
    }

    let base = textureSample(input_texture, tex_sampler, suv);

    // 3. Chromatic ghost: offset RGB channels
    let ghost_amt = intensity * 0.04;
    let r = textureSample(input_texture, tex_sampler, suv + vec2<f32>(ghost_amt, 0.0)).r;
    let b = textureSample(input_texture, tex_sampler, suv - vec2<f32>(ghost_amt, 0.0)).b;

    return vec4<f32>(r, base.g, b, 1.0);
}
