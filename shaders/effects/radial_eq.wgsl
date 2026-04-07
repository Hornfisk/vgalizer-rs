// radial_eq · Polar EQ fan / sonar pulse. 64 thin radial spokes around a
// central pulsing disc with two faint guide rings.

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;
const SPOKES: i32 = 64;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();
    let p = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0);

    let r = length(p);
    let ang = atan2(p.y, p.x);

    var col = vec3<f32>(0.0);

    // Tunables
    let inner_r   = 0.10 + param(1u) * 0.30;     // 0.10..0.40
    let max_len   = 0.30 + param(2u) * 0.50;     // 0.30..0.80
    let thickness = 0.0015 + param(3u) * 0.006;  // 0.0015..0.0075
    let hue       = param(4u);
    let ring_glow = 0.3 + param(5u) * 1.2;

    let spoke_f = (ang + PI) / TAU * f32(SPOKES);
    let s_idx_raw = i32(floor(spoke_f + 0.5));
    let s_idx = ((s_idx_raw % SPOKES) + SPOKES) % SPOKES;

    // 4-fold mirror symmetry
    let h_n = SPOKES / 2;
    let q_n = SPOKES / 4;
    var sym = s_idx;
    if (sym >= h_n) { sym = SPOKES - 1 - sym; }
    if (sym >= q_n) { sym = h_n - 1 - sym; }
    let band_idx = u32(sym % 8);
    let band_v = band(band_idx);

    // Sweep modulator
    let sweep_ang = (globals.time * 0.6) - PI;
    var d_sweep = ang - sweep_ang;
    d_sweep = atan2(sin(d_sweep), cos(d_sweep));
    let sweep_gain = 0.55 + 0.65 * exp(-d_sweep * d_sweep * 4.0);

    let spoke_len = inner_r + band_v * max_len * (0.7 + 0.4 * pls) * sweep_gain;

    let spoke_centre = (f32(s_idx) + 0.5) / f32(SPOKES) * TAU - PI;
    var d_ang = ang - spoke_centre;
    d_ang = atan2(sin(d_ang), cos(d_ang));
    let tang = abs(d_ang) * r;
    let aa = 1.5 / res.y;
    let spoke_w = 1.0 - smoothstep(thickness * 0.4, thickness * 1.6 + aa, tang);

    if (r >= inner_r - 0.002 && r <= spoke_len + 0.002 && spoke_w > 0.001) {
        let cyan  = vec3<f32>(0.20, 0.85, 1.00);
        let mag   = vec3<f32>(1.00, 0.30, 0.85);
        let lime  = vec3<f32>(0.30, 1.00, 0.40);
        var tint = mix(cyan, mag, clamp(hue * 2.0, 0.0, 1.0));
        tint = mix(tint, lime, clamp((hue - 0.5) * 2.0, 0.0, 1.0));
        let white = vec3<f32>(0.85, 0.95, 1.00);

        let tip_d = abs(r - spoke_len);
        let tip_w = exp(-tip_d * 80.0);
        col = col + tint * spoke_w * (0.6 + 0.5 * band_v) * sweep_gain;
        col = col + white * spoke_w * tip_w * (0.6 + 0.6 * pls);
    }

    // Inner pulsing circle
    let circ_d = abs(r - inner_r);
    let circ_w = exp(-circ_d * circ_d * 6000.0);
    col = col + vec3<f32>(0.30, 0.90, 1.00) * circ_w * (0.35 + 0.7 * pls);

    // Guide rings
    let r1 = abs(r - 0.45);
    col = col + vec3<f32>(0.10, 0.40, 0.55) * exp(-r1 * r1 * 2500.0) * 0.4 * ring_glow;
    let r2 = abs(r - 0.78);
    col = col + vec3<f32>(0.06, 0.25, 0.40) * exp(-r2 * r2 * 2500.0) * 0.35 * ring_glow;

    // Cross-hairs
    let cx = exp(-p.x * p.x * 1500.0) * smoothstep(0.0, 0.78, abs(p.y));
    let cy = exp(-p.y * p.y * 1500.0) * smoothstep(0.0, 0.78, abs(p.x));
    col = col + vec3<f32>(0.08, 0.30, 0.40) * (cx + cy) * 0.18;

    // Centre dot
    col = col + vec3<f32>(0.30, 0.90, 1.00) * exp(-r * r * 1200.0) * 0.6;

    // Beat wash
    col = col + vec3<f32>(0.05, 0.20, 0.30) * pls * exp(-r * 1.6);

    col = col * smoothstep(1.2, 0.2, r);
    col = 1.0 - exp(-col * 1.3);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
