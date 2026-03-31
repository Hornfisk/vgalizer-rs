// SpectrumWave: overlapping horizontal sine-wave lines displaced by spectrum.

struct EffectParams { params: array<f32, 16>, seed: f32, _pad: vec3<f32> };
@group(1) @binding(0) var<uniform> fx: EffectParams;

const PI: f32 = 3.14159265359;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_lines  = max(3.0, fx.params[0] + 4.0);
    let amp      = 0.08 + fx.params[1] * 0.08 + globals.level * 0.06;
    let freq     = 2.0 + fx.params[2] * 2.0;
    let pulse    = smooth_pulse();

    let band_f = uv.x * 31.0;
    let band_lo = u32(band_f);
    let band_hi = min(band_lo + 1u, 31u);
    let bv = mix(band(band_lo), band(band_hi), fract(band_f));

    var color = vec3<f32>(0.0);

    for (var i = 0u; i < 8u; i++) {
        let fi = f32(i);
        if fi >= n_lines { break; }

        // Base y position for this line
        let base_y = (fi + 1.0) / (n_lines + 1.0);

        // Sine displacement, audio-driven
        let phase = t * 0.8 + fi * 0.7 + fx.seed * 2.0;
        let disp = amp * bv * sin(uv.x * freq * PI + phase);

        let line_y = base_y + disp;
        let dist = abs(uv.y - line_y);
        let line_thick = 0.003 + globals.level * 0.002 + pulse * 0.002;
        let on_line = smoothstep(line_thick * 1.5, 0.0, dist);

        let col_t = fi / n_lines;
        let line_col = mix(globals.palette_sa.rgb, globals.palette_rb.rgb, col_t);
        color += line_col * on_line * (0.7 + bv * 0.5 + pulse * 0.3);
    }

    return vec4<f32>(color, 1.0);
}
