// SpectrumTerrain: dual mountain silhouette from spectrum, mirrored top/bottom.

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let height_scale = 0.45 + param(0u) * 0.2;
    let glow = 0.4 + param(1u) * 0.3;
    let pulse = smooth_pulse();

    // Sample the spectrum at this x position (interpolated across 32 bands)
    let band_f = uv.x * 31.0;
    let band_lo = u32(band_f);
    let band_hi = min(band_lo + 1u, 31u);
    let band_frac = fract(band_f);
    let bv = mix(band(band_lo), band(band_hi), band_frac);

    let terrain_h = bv * height_scale * (1.0 + pulse * 0.15);

    // Mirror vertically around center
    let y_center = abs(uv.y - 0.5);  // 0 at center, 0.5 at edges

    let on_terrain = step(y_center, terrain_h);

    // Bright edge at terrain boundary
    let edge_dist = abs(y_center - terrain_h);
    let on_edge = smoothstep(0.015, 0.0, edge_dist) * (0.6 + bv * 0.4);
    let glow_val = exp(-edge_dist * 30.0) * glow * bv;

    // Color: brightest at edges, darker fill, gradient across x
    let col_t = uv.x;
    let terrain_col = mix(globals.palette_sa.rgb, globals.palette_sb.rgb, col_t);
    let edge_col = globals.palette_ra.rgb;

    var color = terrain_col * on_terrain * 0.15;
    color += edge_col * on_edge;
    color += terrain_col * glow_val;

    return vec4<f32>(color, 1.0);
}
