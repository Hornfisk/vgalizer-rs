// isoline_field · Animated topographic contour map of a band-driven scalar
// field. Magenta primary with cyan secondary on beat.

const PI: f32 = 3.14159265359;

fn field(p: vec2<f32>) -> f32 {
    let t = globals.time * 0.6;
    var v: f32 = 0.0;

    let k = 2.4;
    v = v + sin(p.x * k + t * 0.7) * 0.35;
    v = v + sin(p.y * k * 1.1 - t * 0.5) * 0.30;
    v = v + sin((p.x + p.y) * k * 0.7 + t * 0.9) * 0.25;
    v = v + sin((p.x - p.y) * k * 1.3 - t * 0.6) * 0.20;

    for (var i: i32 = 0; i < 5; i = i + 1) {
        let fi = f32(i);
        let ang = t * (0.3 + fi * 0.07) + fi * 1.7;
        let rad = 0.55 + 0.15 * sin(t * 0.4 + fi);
        let c = vec2<f32>(cos(ang), sin(ang)) * rad;
        let d = p - c;
        let r2 = dot(d, d);
        let amp = 0.5 + 0.9 * band(u32(i));
        v = v + amp * exp(-r2 * 4.0);
    }

    let pls = smooth_pulse();
    let r = length(p);
    v = v + pls * 0.6 * sin(r * 14.0 - globals.time * 8.0) * exp(-r * 1.5);
    return v;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();
    let p = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0);

    // Tunables
    let spacing  = 0.05 + param(1u) * 0.25;
    let hue      = param(5u);

    let v = field(p);
    let scaled = v / spacing;
    var d_to_iso = abs(fract(scaled) - 0.5) - 0.5;
    d_to_iso = abs(d_to_iso);

    let fw = fwidth(scaled) + 1.0e-5;
    let line_w = 1.0 - smoothstep(0.0, fw * 1.6, d_to_iso);

    let magenta = vec3<f32>(1.00, 0.30, 0.85);
    let cyan    = vec3<f32>(0.30, 0.95, 1.00);
    let amber   = vec3<f32>(1.00, 0.65, 0.18);
    var tint = mix(magenta, cyan, 0.30 + 0.30 * pls);
    tint = mix(tint, amber, hue);

    var col = tint * line_w;
    col = col + vec3<f32>(line_w * line_w) * 0.45;
    col = col + tint * 0.04 * smoothstep(-0.5, 1.5, v);

    let r = length(p);
    col = col + tint * pls * 0.18 * exp(-r * 1.6);
    col = col * smoothstep(1.2, 0.2, r);

    col = 1.0 - exp(-col * 1.5);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
