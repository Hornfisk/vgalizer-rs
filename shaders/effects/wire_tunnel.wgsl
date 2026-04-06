// WireTunnel (wormhole): longitudinal neon stripes flying through an infinite tunnel.
// Pure analytical tunnel — no raymarching. Stripes run along the tunnel length and
// scroll forward to give a strong sense of acceleration and depth.

const PI: f32 = 3.14159265359;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t       = globals.time;
    let fxs     = globals.fx_speed;
    let lvl     = globals.level;
    let pls     = globals.pulse;
    let bass    = band(0u);
    let pulse_s = smooth_pulse();

    // Per-scene randomized params
    let n_stripes  = floor(16.0 + param(0u) * 48.0);   // 16..64
    let twist_rate = (param(1u) - 0.5) * 0.12;         // signed twist amount
    let dash_freq  = 0.5 + param(2u) * 3.5;            // longitudinal dashes per unit z
    let dash_mix   = clamp(param(3u), 0.0, 1.0);       // 0 = continuous, 1 = fully dashed
    let base_thick = 0.012 + param(4u) * 0.018;        // stripe thickness
    let roll_rate  = (param(5u) - 0.5) * 0.6;          // gentle camera roll
    let warp_amt   = 0.06 + param(6u) * 0.14;          // bass lateral warp
    let speed_base = 1.5 + param(7u) * 2.5;            // base forward speed

    // Aspect-correct, centered coords
    let asp = globals.resolution.x / globals.resolution.y;
    var p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    // Bass-driven lateral warp of the tunnel center
    let warp = vec2<f32>(
        sin(t * 0.7) * bass * warp_amt,
        cos(t * 0.9) * bass * warp_amt * 0.6,
    );
    p -= warp;

    // Gentle camera roll
    let ra = t * roll_rate + pls * 0.2;
    let cs = cos(ra);
    let sn = sin(ra);
    p = vec2<f32>(p.x * cs - p.y * sn, p.x * sn + p.y * cs);

    // Tunnel coords
    let r = max(length(p), 0.0008);
    var angle = atan2(p.y, p.x);

    // Depth: r → 0 is far end, r large is near
    let depth = 1.0 / r;

    // Forward scroll velocity, modulated by audio
    let forward_speed = speed_base + lvl * 3.0 + pls * 4.0;
    let z = depth + t * fxs * forward_speed;

    // Slight spiral twist with depth
    angle += z * twist_rate;

    // Longitudinal stripes around the circumference
    let ang_n = angle / (2.0 * PI) * n_stripes;
    let stripe_dist = abs(fract(ang_n + 0.5) - 0.5);

    // Sub-pixel AA: thickness in fract-space scales with screen density of stripes
    // Far away (small r) circumference is tiny → many stripes converge → widen AA.
    let aa_w = fwidth(ang_n) + 0.0005;
    let beat_boost = 1.0 + pulse_s * 1.5;
    let thick = base_thick * beat_boost;
    var stripe = smoothstep(thick + aa_w, max(thick - aa_w, 0.0), stripe_dist);

    // Optional dashed segmentation along z (long neon dashes)
    let dash_raw = 0.5 + 0.5 * sin(z * dash_freq);
    let dash = mix(1.0, smoothstep(0.25, 0.75, dash_raw), dash_mix);
    stripe *= dash;

    // Depth fade: r→0 is far end, fade to black there. r large is near, also fade gently.
    let far_fade  = smoothstep(0.0, 0.45, r);          // kill the bright singularity
    let near_fade = 1.0 - smoothstep(0.9, 1.6, r);     // fade out edges slightly
    let fade = far_fade * near_fade;

    // Perspective brightness falloff along z (acceleration trails)
    let persp = clamp(r * 1.4, 0.0, 1.0);

    // Base neon-white with slight cyan tint
    let neon = vec3<f32>(0.85, 0.95, 1.0);

    var color = neon * stripe * fade * persp * (1.1 + lvl * 0.6 + pulse_s * 0.8);

    // Subtle speed-line shimmer modulated by bands
    let band_idx = u32(clamp((angle / (2.0 * PI) + 0.5) * 32.0, 0.0, 31.0));
    let bv = band(band_idx);
    color += neon * stripe * fade * bv * 0.35;

    // Magenta accent at the very edge on beat
    let edge = smoothstep(1.1, 1.5, r);
    let magenta = vec3<f32>(1.0, 0.25, 0.8);
    color += magenta * edge * pulse_s * 0.6 * stripe;

    // Pure black background — no ambient
    return vec4<f32>(color, 1.0);
}
