// MorphGeo: morphing star-polygons at multiple layers.

const TAU: f32 = 6.28318530718;

fn sd_star_poly(p: vec2<f32>, n: f32, r: f32, k: f32) -> f32 {
    let angle = atan2(p.y, p.x);
    let seg_angle = TAU / n;
    let snapped = round(angle / seg_angle) * seg_angle;
    let a = angle - snapped;
    let even = abs(fract(angle / seg_angle));
    let target_r = mix(r * k, r, step(0.5, even));
    return length(p) - target_r;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_layers = max(2.0, param(0u) + 2.0);
    let base_n   = max(3.0, param(1u) + 3.0);
    let pulse    = smooth_pulse();

    let asp = globals.resolution.x / globals.resolution.y;
    let p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    var color = vec3<f32>(0.0);

    for (var i = 0u; i < 6u; i++) {
        let fi = f32(i);
        if fi >= n_layers { break; }

        let rot = t * (0.2 + fi * 0.07) * (1.0 + globals.level * 0.3);
        let ca = cos(rot); let sa = sin(rot);
        let rp = vec2<f32>(p.x * ca - p.y * sa, p.x * sa + p.y * ca);

        let n_t = fract(t * 0.1 + fi * 0.25 + fx.seed_pad.x);
        let n_lo = base_n + floor(fi * 1.5);
        let n_interp = mix(n_lo, n_lo + 1.0, smoothstep(0.45, 0.55, n_t));

        let radius = 0.15 + fi * 0.06;
        let inner_k = 0.4 + sin(t * 0.3 + fi) * 0.1;

        let d = sd_star_poly(rp, n_interp, radius, inner_k);
        let thick = 0.005 + globals.level * 0.003 + pulse * 0.004;
        let on_outline = smoothstep(thick, 0.0, abs(d));

        let bv = band(u32(fi * 5.0));
        let col_t = fi / n_layers;
        let layer_col = mix(globals.palette_sa.rgb, globals.palette_rb.rgb, col_t);
        color += layer_col * on_outline * (1.0 + bv * 0.5 + pulse * 0.3);
    }

    return vec4<f32>(color, 1.0);
}
