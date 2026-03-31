// Rotation + vertical vibration offset post-pass.

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

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(0) @binding(2) var<uniform> post: PostUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let angle = post.rotation_angle;
    let vib   = post.vibration_y;

    // Short-circuit when idle to save GPU cycles
    if abs(angle) < 0.0001 && abs(vib) < 0.0001 {
        return textureSample(input_texture, tex_sampler, uv);
    }

    // Rotate around center
    let p = uv - 0.5;
    let ca = cos(angle);
    let sa = sin(angle);
    let rp = vec2<f32>(p.x * ca - p.y * sa, p.x * sa + p.y * ca);
    let rotated_uv = rp + 0.5;

    // Vertical vibration (pixel-row shift)
    let shifted_uv = vec2<f32>(rotated_uv.x, rotated_uv.y + vib);

    // Clamp to [0,1] — black border on out-of-range pixels
    if shifted_uv.x < 0.0 || shifted_uv.x > 1.0 || shifted_uv.y < 0.0 || shifted_uv.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    return textureSample(input_texture, tex_sampler, shifted_uv);
}
