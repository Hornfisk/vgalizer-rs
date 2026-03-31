// Shared uniform layout — prepended to every effect fragment shader by the Rust loader.
// Defines GlobalUniforms, EffectParams, and helper functions available to all shaders.

struct GlobalUniforms {
    time: f32,
    dt: f32,
    beat_time: f32,
    fx_speed: f32,

    resolution: vec2<f32>,
    _pad1: vec2<f32>,

    level: f32,
    pulse: f32,
    beat: f32,
    half_beat: f32,

    quarter_beat: f32,
    bpm: f32,
    _pad2: vec2<f32>,

    bands: array<vec4<f32>, 8>,

    palette_sa: vec4<f32>,
    palette_sb: vec4<f32>,
    palette_ra: vec4<f32>,
    palette_rb: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: GlobalUniforms;

// Access band i (0..31)
fn band(i: u32) -> f32 {
    let vi = i / 4u;
    let ci = i % 4u;
    return globals.bands[vi][ci];
}

// Smooth pulse: decays from beat_time
fn smooth_pulse() -> f32 {
    return exp(-globals.beat_time * 3.0);
}

// Hash for procedural noise
fn hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(127.1, 311.7));
    q += dot(q, q + 19.19);
    return fract(q.x * q.y);
}

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
    q += dot(q, q + 19.19);
    return fract(q.x * q.y + q.y * q.z);
}

// SDF helpers
fn sd_circle(p: vec2<f32>, r: f32) -> f32 { return length(p) - r; }

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// EffectParams: per-effect uniform (@group(1) @binding(0)).
// Uses vec4 arrays for proper 16-byte uniform alignment.
// Matches Rust EffectUniforms: [f32;16] + seed:f32 + _pad:[f32;3]
struct EffectParams {
    params: array<vec4<f32>, 4>,  // 64 bytes — 16 f32 params
    seed_pad: vec4<f32>,          // 16 bytes — seed in .x
};
@group(1) @binding(0) var<uniform> fx: EffectParams;

// Access param i (0..15)
fn param(i: u32) -> f32 {
    return fx.params[i / 4u][i % 4u];
}
