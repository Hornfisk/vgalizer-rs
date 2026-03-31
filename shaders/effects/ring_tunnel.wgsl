// RingTunnel: concentric shapes tunneling toward viewer.

const TAU: f32 = 6.28318530718;

fn sd_ring_shape(p: vec2<f32>, r: f32, shape: f32) -> f32 {
    let circ = abs(length(p) - r);
    let sq   = abs(max(abs(p.x), abs(p.y)) - r);
    return mix(circ, sq, clamp(shape, 0.0, 1.0));
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let n_rings  = max(4.0, param(0u) + 4.0);
    let speed    = 0.4 + param(1u) * 0.4;
    let shape    = param(2u);
    let rot_spd  = param(3u) * 0.5;
    let pulse    = smooth_pulse();

    let asp = globals.resolution.x / globals.resolution.y;
    var p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    let ca = cos(t * rot_spd); let sa = sin(t * rot_spd);
    p = vec2<f32>(p.x * ca - p.y * sa, p.x * sa + p.y * ca);

    let audio_speed = speed + globals.level * 0.8 + globals.beat * 0.4;
    var color = vec3<f32>(0.0);

    for (var i = 0u; i < 12u; i++) {
        let fi = f32(i);
        let phase = fract((fi / n_rings) + t * audio_speed * 0.5 + fx.seed_pad.x * 0.1);
        let r = 0.05 + phase * 0.65;
        let thick = 0.005 + (1.0 - phase) * 0.008 + globals.level * 0.004;

        let d = sd_ring_shape(p, r, shape);
        let on_ring = smoothstep(thick, 0.0, abs(d));

        let bv = band(u32(phase * 31.0));
        let col_t = fract(fi * 0.37 + fx.seed_pad.x);
        let ring_col = mix(
            mix(globals.palette_sa.rgb, globals.palette_sb.rgb, col_t),
            globals.palette_ra.rgb, bv * 0.5
        );
        color += ring_col * on_ring * (0.8 + bv * 0.4 + pulse * 0.3);
    }

    return vec4<f32>(color, 1.0);
}
