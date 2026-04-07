// harmonograph · Twin-pendulum harmonograph trace.
// Single-pass distance-to-curve (no ping-pong); compensates with strong halo.

const TAU: f32 = 6.28318530718;
const N: i32 = 384;

fn curve(t: f32) -> vec2<f32> {
    let phase = globals.time * 0.6;

    let A1 = 0.42 + 0.10 * band(0u);
    let A2 = 0.32 + 0.10 * band(2u);
    let f1 = 2.40 + 0.40 * band(1u) + param(0u) * 3.0;
    let f2 = 3.10 + 0.50 * band(3u);
    let p1 = phase * 0.30;
    let p2 = phase * 0.50 + 1.0;
    let d1 = 0.0010;
    let d2 = 0.0014;
    let xx = A1 * sin(f1 * t + p1) * exp(-d1 * t)
           + A2 * sin(f2 * t + p2) * exp(-d2 * t);

    let A3 = 0.40 + 0.09 * band(4u);
    let A4 = 0.30 + 0.09 * band(6u);
    let f3 = 2.55 + 0.50 * band(5u) + param(1u) * 3.0;
    let f4 = 3.85 + 0.45 * band(7u);
    let p3 = phase * 0.42 + 0.5;
    let p4 = phase * 0.36 + 2.0;
    let yy = A3 * sin(f3 * t + p3) * exp(-d1 * t)
           + A4 * sin(f4 * t + p4) * exp(-d2 * t);

    return vec2<f32>(xx, yy);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();

    var p = (uv - vec2<f32>(0.5)) * 2.0;
    p.x = p.x * aspect;
    p.y = -p.y;

    let halo_w = 0.0020 + param(2u) * 0.005;
    let glow_k = 0.5 + param(3u) * 1.5;
    let hue    = param(4u);

    var min_d2: f32 = 1.0e9;
    for (var i: i32 = 0; i < N; i = i + 1) {
        let t = f32(i) / f32(N) * 6.0 * TAU;
        let q = curve(t);
        let dd = p - q;
        min_d2 = min(min_d2, dot(dd, dd));
    }

    let pix = 2.0 / min(res.x, res.y);
    let core = exp(-min_d2 / (pix * pix * 1.4));
    let glow = exp(-min_d2 / halo_w) * glow_k;

    let amber   = vec3<f32>(1.00, 0.65, 0.18);
    let cyan    = vec3<f32>(0.30, 0.85, 1.00);
    let tint    = mix(amber, cyan, hue);

    var col = vec3<f32>(core) * 1.1 + tint * glow * 0.65;
    col = col + tint * pls * 0.10 * exp(-length(p) * 1.5);

    col = 1.0 - exp(-col * 1.5);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
