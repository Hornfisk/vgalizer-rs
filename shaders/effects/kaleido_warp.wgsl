// kaleido_warp · N-fold rotational kaleidoscope of a thin-line seed pattern.
// Distinct from `kaleido` — pure thin-line geometry, no source image.

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

fn seed(p: vec2<f32>) -> f32 {
    let r = length(p);
    let ang = atan2(p.y, p.x);
    let aa = 1.5 / globals.resolution.y;

    var v: f32 = 0.0;

    for (var i: i32 = 0; i < 5; i = i + 1) {
        let fi = f32(i);
        var r0 = 0.12 + fi * 0.16 + 0.05 * sin(globals.time * 0.8 + fi);
        r0 = r0 + 0.06 * band(u32(i));
        let d = abs(r - r0);
        v = v + (1.0 - smoothstep(0.001, 0.004 + aa, d)) * (0.55 + 0.45 * band(u32(i)));
    }

    let spokes = 7.0;
    var a2 = (ang + globals.time * 0.15) - floor((ang + globals.time * 0.15) / (TAU / spokes)) * (TAU / spokes);
    a2 = abs(a2 - (TAU / spokes) * 0.5);
    let spoke_d = a2 * r;
    v = v + (1.0 - smoothstep(0.002, 0.006 + aa, spoke_d)) * (0.5 + 0.5 * band(3u));

    let diag = abs(p.y - sin(p.x * 6.0 + globals.time * 1.3) * 0.18);
    v = v + (1.0 - smoothstep(0.002, 0.006 + aa, diag)) * 0.6 * (0.5 + band(5u));

    v = v + exp(-r * r * 600.0) * 0.7;
    return v;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();
    var p = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0);

    // Tunables
    let segs_base  = 4.0 + param(0u) * 12.0;        // 4..16
    let rot_speed  = (param(1u) - 0.5) * 0.6;
    let warp_amt   = param(2u) * 1.5;
    let hue_shift  = param(4u);

    // Bass nudges segment count smoothly
    let segs = segs_base + band(0u) * 4.0;

    // Global rotation
    let rot = globals.time * (0.20 + rot_speed) + pls * 0.4;
    let cs = cos(rot);
    let sn = sin(rot);
    p = mat2x2<f32>(cs, sn, -sn, cs) * p;

    let r = length(p);
    let ang = atan2(p.y, p.x);

    let wedge = TAU / segs;
    var a = (ang + PI) - floor((ang + PI) / wedge) * wedge;
    a = abs(a - wedge * 0.5);

    let warp = 0.55 * sin(r * 6.0 - globals.time * 1.4) * (0.5 + band(2u)) * warp_amt;
    a = a + warp * 0.15;

    let fp = vec2<f32>(cos(a) * r, sin(a) * r);

    let v = seed(fp);

    let cA = vec3<f32>(0.20, 0.95, 1.00);
    let cB = vec3<f32>(1.00, 0.30, 0.85);
    let cC = vec3<f32>(0.40, 1.00, 0.30);
    let ph = fract(globals.time * 0.07 + hue_shift);
    let tint = mix(mix(cA, cB, ph), cC, 0.5 - 0.5 * cos(ph * TAU));

    var col = vec3<f32>(v) * tint;
    col = col + vec3<f32>(v * v) * 0.6;
    col = col + tint * exp(-r * 4.0) * pls * 0.6;
    col = col * smoothstep(1.1, 0.2, length(p));

    col = 1.0 - exp(-col * 1.4);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
