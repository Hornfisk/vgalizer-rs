// WarpGrid: 3D sine-wave deformed grid.

const PI: f32 = 3.14159265359;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_cols  = max(4.0, param(0u) + 6.0);
    let n_rows  = max(4.0, param(1u) + 6.0);
    let amp     = 0.05 + param(2u) * 0.08 + globals.level * 0.06;
    let freq    = 2.0 + param(3u) * 3.0;
    let pulse   = smooth_pulse();

    let asp = globals.resolution.x / globals.resolution.y;
    let p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    let px_warped = p.x + amp * sin(p.y * freq * PI + t * 1.3);
    let py_warped = p.y + amp * sin(p.x * freq * PI + t * 1.1);

    let cell_w = asp / n_cols;
    let frac_x = fract((px_warped + asp * 0.5) / cell_w);
    let dist_v = min(frac_x, 1.0 - frac_x) * cell_w;

    let cell_h = 1.0 / n_rows;
    let frac_y = fract((py_warped + 0.5) / cell_h);
    let dist_h = min(frac_y, 1.0 - frac_y) * cell_h;

    let thick = 0.003 + globals.level * 0.002 + pulse * 0.003;
    let on_v = smoothstep(thick, 0.0, dist_v);
    let on_h = smoothstep(thick, 0.0, dist_h);

    let dist_center = length(p);
    let fade = 1.0 - smoothstep(0.3, 0.7, dist_center);

    let band_idx = u32(clamp(uv.x * 32.0, 0.0, 31.0));
    let bv = band(band_idx);
    let grid_col = mix(globals.palette_sa.rgb, globals.palette_sb.rgb, uv.y);
    let audio_col = mix(grid_col, globals.palette_ra.rgb, bv * 0.6);

    let intensity = (on_v + on_h) * fade * (0.8 + globals.level * 0.4);
    return vec4<f32>(audio_col * intensity, 1.0);
}
