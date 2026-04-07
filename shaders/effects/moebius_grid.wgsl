// moebius_grid · Cartesian grid mapped through w = (a·z + b)/(c·z + d).
// Per-pixel inverse Möbius, then distance-to-grid in z-space with fwidth AA.

const PI: f32 = 3.14159265359;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

fn cdiv(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let den = dot(b, b) + 1.0e-6;
    return vec2<f32>(a.x * b.x + a.y * b.y, a.y * b.x - a.x * b.y) / den;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();

    var w = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0);

    // Tunables
    let density    = 2.0 + param(0u) * 8.0;       // 2..10
    let pole_max   = 0.10 + param(1u) * 0.50;     // 0.10..0.60
    let drift_speed= 0.1 + param(2u) * 1.0;
    let rot_speed  = (param(3u) - 0.5) * 0.6;
    let hue        = param(4u);

    // Global rotation
    let rot = globals.time * (0.15 + rot_speed);
    let cs = cos(rot);
    let sn = sin(rot);
    w = mat2x2<f32>(cs, sn, -sn, cs) * w;

    // Drifting Möbius coefficients
    let t = globals.time * drift_speed;
    let a = vec2<f32>(1.0, 0.0);
    let b = vec2<f32>(0.20 * sin(t * 0.7), 0.20 * cos(t * 0.9));
    let bass = band(0u) + 0.5 * band(1u);
    let pole_mag = 0.10 + (pole_max - 0.10) * bass + 0.10 * pls;
    let c = vec2<f32>(pole_mag * cos(t), pole_mag * sin(t));
    let d = vec2<f32>(1.0 + 0.10 * sin(t * 1.3), 0.10 * cos(t * 1.1));

    let num = cmul(d, w) - b;
    let den = -cmul(c, w) + a;
    let z = cdiv(num, den);

    // Major grid
    let g = z * density;
    let fr = abs(fract(g) - vec2<f32>(0.5));
    let fw = fwidth(g) + vec2<f32>(1.0e-5);
    let line_x = 1.0 - smoothstep(0.0, fw.x * 1.4, fr.x);
    let line_y = 1.0 - smoothstep(0.0, fw.y * 1.4, fr.y);

    // Fine subgrid
    let g2 = z * density * 5.0;
    let fr2 = abs(fract(g2) - vec2<f32>(0.5));
    let fw2 = fwidth(g2) + vec2<f32>(1.0e-5);
    let sub_x = (1.0 - smoothstep(0.0, fw2.x * 1.4, fr2.x)) * 0.35;
    let sub_y = (1.0 - smoothstep(0.0, fw2.y * 1.4, fr2.y)) * 0.35;

    let stretch = clamp(1.0 - 0.015 * length(fw), 0.15, 1.0);
    let lines = max(max(line_x, line_y), max(sub_x, sub_y)) * stretch;

    let green = vec3<f32>(0.30, 1.00, 0.45);
    let cyan  = vec3<f32>(0.30, 0.95, 1.00);
    let amber = vec3<f32>(1.00, 0.65, 0.18);
    var tint = mix(green, cyan, clamp(hue * 2.0, 0.0, 1.0));
    tint = mix(tint, amber, clamp((hue - 0.5) * 2.0, 0.0, 1.0));
    tint = mix(tint, cyan, 0.30 * pls);

    var col = tint * lines;
    col = col + vec3<f32>(lines * lines) * 0.55;

    let pole_d = length(den);
    col = col + tint * 0.15 * exp(-pole_d * 6.0);

    let vd = length(w);
    col = col * smoothstep(1.2, 0.2, vd);

    col = 1.0 - exp(-col * 1.4);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
