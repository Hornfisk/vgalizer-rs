// Trail blend: dim previous trail buffer, add new effect output additively.

struct PostUniforms {
    trail_alpha: f32,
    glitch_intensity: f32,
    vga_intensity: f32,
    vga_ca: f32,
    vga_noise: f32,
    vga_sync: f32,
    rotation_angle: f32,
    vibration_y: f32,
    strobe_alpha: f32,
    strobe_r: f32,
    strobe_g: f32,
    strobe_b: f32,
    mirror_mode: u32,
    mirror_alpha: f32,
    mirror_count: u32,
    mirror_spread: f32,
};

@group(0) @binding(0) var trail_texture: texture_2d<f32>;
@group(0) @binding(1) var effect_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> post: PostUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let trail = textureSample(trail_texture, tex_sampler, uv);
    let curr  = textureSample(effect_texture, tex_sampler, uv);
    let decay = post.trail_alpha / 255.0;
    // Dim previous, add current (additive blend for glow/trail effect)
    return vec4<f32>(trail.rgb * (1.0 - decay) + curr.rgb, 1.0);
}
