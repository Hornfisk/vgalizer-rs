// Strobe: beat-synced color flash overlay.

struct PostUniforms {
    trail_alpha: f32, glitch_intensity: f32, vga_intensity: f32, vga_ca: f32,
    vga_noise: f32, vga_sync: f32, rotation_angle: f32, vibration_y: f32,
    strobe_alpha: f32, strobe_r: f32, strobe_g: f32, strobe_b: f32,
    mirror_mode: u32, mirror_alpha: f32, mirror_count: u32, mirror_spread: f32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(0) @binding(2) var<uniform> post: PostUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let base = textureSample(input_texture, tex_sampler, uv);
    let alpha = post.strobe_alpha;
    if alpha <= 0.0 { return base; }
    let flash = vec3<f32>(post.strobe_r, post.strobe_g, post.strobe_b);
    return vec4<f32>(mix(base.rgb, flash, alpha), 1.0);
}
