// Mirror/kaleido post-processing.

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

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

fn sample(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(input_texture, tex_sampler, uv);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let mode = post.mirror_mode;

    if mode == 0u {
        // No mirror
        return sample(uv);
    } else if mode == 1u {
        // Horizontal mirror
        let muv = vec2<f32>(1.0 - uv.x, uv.y);
        let alpha = post.mirror_alpha / 255.0;
        return sample(uv) * (1.0 - alpha * 0.5) + sample(muv) * alpha * 0.5;
    } else if mode == 2u {
        // Vertical mirror
        let muv = vec2<f32>(uv.x, 1.0 - uv.y);
        let alpha = post.mirror_alpha / 255.0;
        return sample(uv) * (1.0 - alpha * 0.5) + sample(muv) * alpha * 0.5;
    } else if mode == 3u {
        // Quad mirror (4-way)
        let fuv = vec2<f32>(abs(uv.x * 2.0 - 1.0) * 0.5 + 0.5, abs(uv.y * 2.0 - 1.0) * 0.5 + 0.5);
        return sample(fuv);
    } else {
        // Kaleido: fold into wedge
        let p = uv - 0.5;
        let n = max(2u, post.mirror_count);
        let nf = f32(n);
        var angle = atan2(p.y, p.x);
        let r = length(p);
        let sector = TAU / nf;
        angle = fract(angle / sector + 0.5) * sector;
        // Mirror within sector
        let half = sector * 0.5;
        angle = abs(angle - half) + half;
        let q = vec2<f32>(cos(angle), sin(angle)) * r;
        return sample(q + 0.5);
    }
}
