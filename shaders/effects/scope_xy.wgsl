// scope_xy · Vector oscilloscope (Lissajous). Distance-to-curve rendering
// without ping-pong trail (engine is single-pass). Strong analytical halo.

const TAU: f32 = 6.28318530718;
const N: i32 = 256;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();

    var p = (uv - vec2<f32>(0.5)) * 2.0;
    p.x = p.x * aspect;
    // Flip y so up matches preview orientation
    p.y = -p.y;

    // Tunables
    let ratio_a = 1.5 + param(0u) * 7.0;     // 1.5..8.5
    let ratio_b = 1.5 + param(1u) * 7.0;
    let amp_p   = 0.20 + param(2u) * 0.40;   // 0.20..0.60
    let halo    = 0.0015 + param(3u) * 0.005;// halo radius²
    let hue     = param(4u);
    let glow_k  = 0.5 + param(5u) * 1.5;

    let a = ratio_a + band(1u) * 1.5;
    let b = ratio_b + band(3u) * 1.5;
    let ph = globals.time * 0.35;
    let amp = amp_p * (1.0 + 0.25 * pls);

    // Sample the curve and find the minimum distance to this pixel
    var min_d2: f32 = 1.0e9;
    for (var i: i32 = 0; i < N; i = i + 1) {
        let t = f32(i) / f32(N) * TAU;
        let x = sin(a * t + ph) * amp
              + sin(a * t * 2.0 + ph * 1.3) * amp * 0.18 * band(2u);
        let y = sin(b * t + ph * 0.7) * amp
              + sin(b * t * 2.0 - ph * 0.9) * amp * 0.18 * band(5u);
        let q = vec2<f32>(x, y);
        let dd = p - q;
        let d2 = dot(dd, dd);
        min_d2 = min(min_d2, d2);
    }

    let pix = 2.0 / min(res.x, res.y);
    let core = exp(-min_d2 / (pix * pix * 1.5));
    let glow = exp(-min_d2 / halo) * glow_k;

    // Tint
    let cyan    = vec3<f32>(0.30, 0.85, 1.00);
    let magenta = vec3<f32>(1.00, 0.30, 0.85);
    let amber   = vec3<f32>(1.00, 0.65, 0.18);
    var tint = mix(cyan, magenta, clamp(hue * 2.0, 0.0, 1.0));
    tint = mix(tint, amber, clamp((hue - 0.5) * 2.0, 0.0, 1.0));

    var col = vec3<f32>(core) + tint * glow * 0.55;
    col = col + tint * pls * 0.12 * exp(-length(p) * 1.5);

    col = 1.0 - exp(-col * 1.6);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
