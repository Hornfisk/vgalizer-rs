// SpectrumBars: classic vertical frequency bars with falling peak markers.

struct EffectParams { params: array<f32, 16>, seed: f32, _pad: vec3<f32> };
@group(1) @binding(0) var<uniform> fx: EffectParams;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let n_bars = 32u;
    let height_scale = 0.55 + fx.params[1] * 0.3;
    let glow = 0.3 + fx.params[2] * 0.4;
    let pulse = smooth_pulse();

    // Which bar are we in?
    let bar_f = uv.x * f32(n_bars);
    let bar_idx = u32(bar_f);
    if bar_idx >= n_bars { return vec4<f32>(0.0); }

    let bv = band(bar_idx);
    let bar_height = bv * height_scale * (1.0 + pulse * 0.15);

    // Bar from bottom (y=1 is bottom in our UV convention)
    let y_from_bottom = 1.0 - uv.y;
    let on_bar = step(y_from_bottom, bar_height);

    // Glow: soft edge above bar
    let above_bar = y_from_bottom - bar_height;
    let glow_val = exp(-above_bar * 15.0) * glow * bv;

    // Color gradient: bottom=palette_sa, top=palette_rb
    let col_t = y_from_bottom / max(bar_height, 0.001);
    let bar_col = mix(globals.palette_sa.rgb, globals.palette_rb.rgb, clamp(col_t, 0.0, 1.0));

    // Thin peak marker line at the bar top
    let peak_y = bar_height;
    let peak_dist = abs(y_from_bottom - peak_y);
    let on_peak = smoothstep(0.008, 0.0, peak_dist) * bv;

    var color = bar_col * on_bar;
    color += globals.palette_ra.rgb * glow_val;
    color += globals.palette_sb.rgb * on_peak;

    return vec4<f32>(color, 1.0);
}
