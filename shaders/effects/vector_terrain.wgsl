// vector_terrain · Geiss/Milkdrop wireframe waterfall.
// Hidden-line painter's algorithm; 36 rows of bands projected in perspective.
// Reds at low altitudes, greens at peaks. Pure thin-line on black.

const PI: f32 = 3.14159265359;
const ROWS: i32 = 36;

fn sample_band_row(row: i32, col_f: f32) -> f32 {
    // Synthesise a per-row spectrum from the live bands plus a row-dependent
    // time offset, so older rows look like an earlier snapshot of the spectrum.
    let t = globals.time - f32(row) * 0.10;
    var h: f32 = 0.0;
    for (var b: i32 = 0; b < 8; b = b + 1) {
        let pos = (f32(b) - 3.5) * 0.95;
        let dx = col_f - pos;
        let amp = band(u32(b)) * (0.55 + 0.55 * sin(t * (0.8 + f32(b) * 0.13) + f32(b) * 1.1));
        h = h + max(amp, 0.0) * exp(-dx * dx * 0.45);
    }
    return h;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();

    // 0..1 with origin bottom-left for projection math
    let p = vec2<f32>(uv.x, 1.0 - uv.y);

    // Tunables (param 0..5)
    let horizon     = 0.50 + param(0u) * 0.30;            // 0.50..0.80
    let height_mul  = 0.45 + param(2u) * 1.10;            // 0.45..1.55
    let scroll_mul  = 0.4  + param(3u) * 1.6;             // 0.4..2.0
    let wave_amt    = param(4u) * 0.40;                   // 0..0.40
    let beat_kick   = 0.5  + param(5u) * 0.7;             // 0.5..1.2

    if (p.y > horizon + 0.001) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    var col = vec3<f32>(0.0);
    var sky_min = horizon;

    for (var i: i32 = 0; i < ROWS; i = i + 1) {
        let z_row = 0.7 + f32(i) * 0.36;
        let wx_scale = z_row * 0.85 * aspect * 0.5;
        let world_x = (p.x - 0.5) * 2.0 * wx_scale;

        var h = sample_band_row(i, world_x);
        // Travelling secondary ridges
        h = h + 0.18 * sin(world_x * 3.5 + f32(i) * 0.7 - globals.time * 2.3 * scroll_mul);
        h = h + 0.10 * sin(world_x * 7.0 - f32(i) * 0.4 + globals.time * 3.1 * scroll_mul);
        // Central envelope (taller in the middle)
        h = h * exp(-world_x * world_x * 0.085);
        // Travelling-wave wobble
        h = h * (0.65 + 0.55 * sin(f32(i) * 0.45 - globals.time * 2.6 * scroll_mul));
        // Beat punch
        h = h * (0.55 + 0.95 * (pls * beat_kick + 0.30));
        h = max(h, 0.0) * height_mul;
        // Allow params to add a wave amount manually too
        h = h * (1.0 + wave_amt * sin(world_x * 6.0 + globals.time));

        // Ground projection
        let y_line = horizon - (0.5 + h * 1.05) / z_row;
        if (y_line < 0.0) { continue; }

        let d = abs(p.y - y_line);
        let aa = 1.4 / res.y;
        let w = 1.0 - smoothstep(aa * 0.5, aa * 2.5, d);
        if (w < 0.001) { continue; }

        // Hidden-line: skip rows behind closer silhouette
        if (y_line > sky_min - 0.0005) { continue; }

        let h_n = clamp(h * 1.6, 0.0, 1.0);
        var c = mix(vec3<f32>(1.00, 0.18, 0.06), vec3<f32>(0.18, 1.00, 0.30), h_n);
        c = c + vec3<f32>(0.6, 0.7, 0.6) * pow(h_n, 3.0) * 0.5;

        let fade = 1.0 - smoothstep(2.0, 14.0, z_row);
        col = max(col, c * w * fade);
        sky_min = min(sky_min, y_line);
    }

    col = col * (1.0 + pls * 0.25);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
