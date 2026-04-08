// LineMoire: interfering line fields with lens warp.

const PI: f32 = 3.14159265359;

fn rot(a: f32) -> mat2x2<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat2x2<f32>(c, -s, s, c);
}

fn aa_line(d: f32, half_width: f32, aa: f32) -> f32 {
    return 1.0 - smoothstep(half_width - aa, half_width + aa, d);
}

fn line_field(p: vec2<f32>, dir: vec2<f32>, spacing: f32, half_width: f32, aa: f32) -> f32 {
    let coord = dot(p, dir);
    let m = coord / spacing;
    let frac_v = m - floor(m + 0.5);
    let d = abs(frac_v) * spacing;
    return aa_line(d, half_width, aa);
}

fn lens_warp(p: vec2<f32>, strength: f32) -> vec2<f32> {
    let r = length(p);
    let k = strength / (0.25 + r * r * 1.4);
    return p - normalize(p + vec2<f32>(1e-6)) * k * 0.22;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;

    let asp = globals.resolution.x / globals.resolution.y;
    var p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    let breath = 1.0 + 0.06 * sin(t * 0.6) + 0.03 * globals.level;
    p *= breath;

    let angle_jitter = (param(0u) - 0.5) * 0.8;
    let base_angle = t * 0.07 + angle_jitter;
    p = rot(base_angle) * p;

    let bass = clamp(band(0u) * 0.6 + band(1u) * 0.4, 0.0, 1.0);
    let pulse = smooth_pulse();
    let lens_k = 0.3 + 0.35 * bass + 0.6 * pulse;
    let wp = lens_warp(p, lens_k);

    let kick = pulse * 0.35;
    let base_offset = 0.045 + 0.025 * sin(t * 0.23) + 0.02 * band(3u);
    let offset = base_offset + kick;

    let spacing_mod = 0.85 + param(1u) * 0.5;
    let spacing  = (0.042 + 0.006 * sin(t * 0.31)) * spacing_mod;
    let spacing2 = spacing * (1.0 + 0.03 + 0.015 * sin(t * 0.17));

    let px_to_uv = 1.0 / min(globals.resolution.x, globals.resolution.y);
    let half_width = 0.32 * px_to_uv;
    let aa = 0.9 * px_to_uv;

    let ang_a = offset * 0.5 + (param(2u) - 0.5) * 0.6;
    let ang_b = offset * 1.8 + (param(3u) - 0.5) * 0.6;
    let ang_c = -offset * 1.1 + 0.08;

    let dir_a = vec2<f32>(cos(ang_a), sin(ang_a));
    let dir_b = vec2<f32>(cos(ang_b), sin(ang_b));
    let dir_c = vec2<f32>(cos(ang_c), sin(ang_c));

    let a_val = line_field(wp, dir_a, spacing, half_width, aa);
    let b_val = line_field(wp, dir_b, spacing2, half_width, aa);
    let c_val = line_field(wp * (1.0 + 0.05 * sin(t * 0.4)), dir_c, spacing * 1.05, half_width * 1.6, aa);

    var white = max(a_val, b_val);
    let interfere = a_val * b_val;
    white = clamp(white + interfere * 0.6, 0.0, 1.0);

    let vig = smoothstep(1.15, 0.25, length(p));
    white *= mix(0.55, 1.0, vig);

    let primary = vec3<f32>(0.92, 0.98, 1.0) * white;

    let accent_amt = clamp(0.5 + 0.5 * pulse, 0.0, 1.0);
    let accent = vec3<f32>(1.0, 0.15, 0.55) * c_val * accent_amt;

    let col = clamp(primary + accent, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}
