// Kaleido: radial spokes + concentric rings.

struct EffectParams { params: array<f32, 16>, seed: f32, _pad: vec3<f32> };
@group(1) @binding(0) var<uniform> fx: EffectParams;

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_spokes = max(3.0, fx.params[0] + 3.0);
    let n_rings  = max(3.0, fx.params[1] + 3.0);
    let rot_speed = 0.3 + fx.params[2] * 0.5;
    let pulse = smooth_pulse();

    let asp = globals.resolution.x / globals.resolution.y;
    var p = (uv - 0.5) * vec2<f32>(asp, 1.0);
    let r = length(p);
    let max_r = 0.55;

    // Dual rotation (inner/outer)
    var angle = atan2(p.y, p.x) + t * rot_speed;
    let angle2 = atan2(p.y, p.x) - t * rot_speed * 0.6;

    // Spokes: distance from nearest spoke angle
    let spoke_gap = TAU / n_spokes;
    let spoke_dist = abs(fract(angle / spoke_gap + 0.5) - 0.5) * spoke_gap * r;
    let spoke_thick = 0.004 + globals.level * 0.003 + pulse * 0.003;
    let on_spoke = smoothstep(spoke_thick, 0.0, spoke_dist);

    // Rings: distance from nearest ring
    let ring_gap = max_r / n_rings;
    let ring_dist = abs(fract(r / ring_gap + t * 0.1) - 0.5) * ring_gap;
    let ring_thick = 0.003 + globals.level * 0.002;
    let on_ring = smoothstep(ring_thick, 0.0, ring_dist) * step(r, max_r);

    // Audio-driven radial bands
    let band_idx = u32(r / max_r * 31.0);
    let band_val = band(clamp(band_idx, 0u, 31u));
    let audio_glow = band_val * 0.3 * smoothstep(max_r, 0.0, r);

    var color = vec3<f32>(0.0);
    // Alternate spoke/ring colors
    let spoke_col = mix(globals.palette_sa.rgb, globals.palette_ra.rgb, fract(angle / spoke_gap));
    let ring_col  = mix(globals.palette_sb.rgb, globals.palette_rb.rgb, fract(r / ring_gap));

    color += spoke_col * on_spoke * (1.0 + pulse * 0.5);
    color += ring_col  * on_ring  * (1.0 + globals.level * 0.3);
    color += globals.palette_sa.rgb * audio_glow;

    return vec4<f32>(color, 1.0);
}
