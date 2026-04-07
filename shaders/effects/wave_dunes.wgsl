// wave_dunes · Stacked waveform dunes scrolling toward a red horizon.
// Each row's waveform is computed directly from the live bands with a
// row-dependent time delay, so motion is alive from frame 1.

const PI: f32 = 3.14159265359;
const ROWS: i32 = 40;

fn wave_at(i: i32, x: f32) -> f32 {
    let row_delay = f32(i) * 0.085;
    let t = globals.time - row_delay;
    var w: f32 = 0.0;
    for (var b: i32 = 0; b < 8; b = b + 1) {
        let fr = (f32(b) + 1.0) * 1.4;
        let phase = f32(b) * 1.7 + t * (0.6 + f32(b) * 0.18);
        w = w + band(u32(b)) * sin(x * fr * PI + phase);
    }
    return w * 0.18;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();
    let p = vec2<f32>(uv.x, 1.0 - uv.y);
    let x = (p.x - 0.5) * 2.0 * aspect;

    // Tunables
    let amp_p     = 0.04 + param(0u) * 0.10;
    let horizon_y = 0.70 + param(1u) * 0.25;
    let perspect  = 1.2 + param(2u) * 1.6;
    let bottom_y  = 0.04 + param(3u) * 0.12;
    let row_thick = 0.008 + param(4u) * 0.012;
    let beat_amp  = 0.5 + param(5u) * 0.7;

    var col = vec3<f32>(0.0);
    let aa = 1.4 / res.y;

    for (var i: i32 = 0; i < ROWS; i = i + 1) {
        let ti = f32(i) / f32(ROWS);
        let y_row = mix(bottom_y, horizon_y, 1.0 - pow(1.0 - ti, perspect));

        let w = wave_at(i, x);
        let amp = mix(0.075, 0.012, ti) + amp_p * (1.0 - ti);
        let y_line = y_row + w * amp;

        let d = abs(p.y - y_line);
        if (d > row_thick * 1.4) { continue; }
        let w_l = 1.0 - smoothstep(aa * 0.4, aa * 1.8, d);
        if (w_l < 0.001) { continue; }

        let c_front = vec3<f32>(1.00, 1.00, 1.00);
        let c_mid   = vec3<f32>(1.00, 0.55, 0.10);
        let c_back  = vec3<f32>(0.55, 0.05, 0.00);
        var c: vec3<f32>;
        if (ti < 0.5) {
            c = mix(c_front, c_mid, ti * 2.0);
        } else {
            c = mix(c_mid, c_back, (ti - 0.5) * 2.0);
        }

        let brighten = 1.0 + pls * (1.0 - ti) * beat_amp;
        col = max(col, c * w_l * brighten);
    }

    let hori_d = abs(p.y - horizon_y);
    col = col + vec3<f32>(0.30, 0.04, 0.02) * exp(-hori_d * 60.0) * 0.4;

    let vd = length(p - vec2<f32>(0.5));
    col = col * smoothstep(1.0, 0.2, vd);

    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
