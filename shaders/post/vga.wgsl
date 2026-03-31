// VGA: chromatic aberration + film grain + sync jitter.

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

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
    q += dot(q, q + 19.19);
    return fract(q.x * q.y + q.y * q.z);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let intensity = post.vga_intensity;
    if intensity <= 0.0 { return textureSample(input_texture, tex_sampler, uv); }

    var suv = uv;

    // Sync jitter: random row displacement
    let row = floor(uv.y * globals.resolution.y / 4.0);
    let jitter_noise = hash3(vec3<f32>(row, floor(globals.time * 8.0), 0.0));
    if jitter_noise < post.vga_sync * intensity {
        suv.x = fract(suv.x + (jitter_noise - 0.5) * 0.05 * intensity);
    }

    // Chromatic aberration: shift R and B channels
    let ca = post.vga_ca / globals.resolution.x * intensity;
    let r = textureSample(input_texture, tex_sampler, suv + vec2<f32>(ca, 0.0)).r;
    let g = textureSample(input_texture, tex_sampler, suv).g;
    let b = textureSample(input_texture, tex_sampler, suv - vec2<f32>(ca, 0.0)).b;

    var color = vec3<f32>(r, g, b);

    // Film grain
    let grain = hash3(vec3<f32>(uv * globals.resolution, globals.time * 60.0));
    color += (grain - 0.5) * post.vga_noise * intensity;

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
