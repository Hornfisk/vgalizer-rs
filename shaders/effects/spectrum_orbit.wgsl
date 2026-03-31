// SpectrumOrbit: radial frequency bars in a ring (radar-style).

struct EffectParams { params: array<f32, 16>, seed: f32, _pad: vec3<f32> };
@group(1) @binding(0) var<uniform> fx: EffectParams;

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_bars    = 32u;
    let inner_r   = 0.18 + fx.params[0] * 0.05;
    let bar_len   = 0.22 + fx.params[1] * 0.1;
    let rot_speed = fx.params[2] * 0.3;
    let pulse     = smooth_pulse();

    let asp = globals.resolution.x / globals.resolution.y;
    let p   = (uv - 0.5) * vec2<f32>(asp, 1.0);
    let r   = length(p);
    var angle = atan2(p.y, p.x) + t * rot_speed;
    if angle < 0.0 { angle += TAU; }

    // Which angular bar are we closest to?
    let bar_angle = TAU / f32(n_bars);
    let bar_idx = u32(angle / bar_angle) % n_bars;
    let bar_f = f32(bar_idx);

    let bv = band(bar_idx);
    let bar_max_r = inner_r + bv * bar_len * (1.0 + pulse * 0.2);

    // Are we within the bar (radially)?
    let in_radial = step(inner_r, r) * step(r, bar_max_r);
    // Are we in the correct angular slice?
    let frac_in_slice = fract(angle / bar_angle);
    let slice_thick = 0.7;
    let in_angular = smoothstep(0.0, 0.15, frac_in_slice) * smoothstep(1.0, 0.85, frac_in_slice);

    let on_bar = in_radial * in_angular;

    // Tip dot at the bar end
    let tip_r = bar_max_r;
    let to_tip = abs(r - tip_r);
    let on_tip = smoothstep(0.012, 0.0, to_tip) * in_angular * bv;

    // Glow beyond bar
    let beyond = r - bar_max_r;
    let glow = exp(-beyond * 20.0) * bv * in_angular * 0.3;

    // Inner ring
    let on_ring = smoothstep(0.004, 0.0, abs(r - inner_r));

    let col_t = bar_f / f32(n_bars);
    let bar_col = mix(globals.palette_sa.rgb, globals.palette_sb.rgb, col_t);
    let tip_col = globals.palette_ra.rgb;

    var color = bar_col * on_bar * (0.8 + bv * 0.4);
    color += tip_col * on_tip;
    color += globals.palette_ra.rgb * glow;
    color += globals.palette_sb.rgb * on_ring * 0.5;

    return vec4<f32>(color, 1.0);
}
