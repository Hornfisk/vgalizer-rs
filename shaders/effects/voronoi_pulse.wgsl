// VoronoiPulse: animated Voronoi edge network. Seed points drift continuously
// via per-seed sin/cos orbits + low-frequency value-noise drift, so the field
// never repeats and never snaps. Beat energy adds a smooth decaying wobble to
// each seed (NOT a jitter trigger).

const PI: f32 = 3.14159265359;
const NUM_POINTS: u32 = 42u;

fn hash1(i: f32) -> f32 {
    return fract(sin(i * 45.164) * 43758.5453);
}

fn hash2(i: f32) -> vec2<f32> {
    return fract(sin(vec2<f32>(i * 12.9898, i * 78.233)) * 43758.5453);
}

// 1D value noise — smooth, continuous, non-repeating drift channel.
fn vnoise(x: f32) -> f32 {
    let i = floor(x);
    var f = fract(x);
    let a = hash1(i);
    let b = hash1(i + 1.0);
    f = f * f * (3.0 - 2.0 * f);
    return mix(a, b, f);
}

// Continuous seed position. `wobble` is a smooth, decaying envelope from the
// beat — it modulates orbit phase/amplitude, never snaps.
fn seed_pos(idx: u32, t: f32, wobble: f32, traj: f32) -> vec2<f32> {
    let fi = f32(idx);
    let h = hash2(fi + 1.0);
    let base = vec2<f32>(h.x * 2.4 - 1.2, h.y * 2.0 - 1.0);

    // Per-seed orbit speeds/radii — different per scene via traj.
    let spd = 0.22 + h.x * 0.45 + traj * 0.08;
    let rad = 0.10 + h.y * 0.18;
    let ph  = fi * 1.7 + traj * 2.3;

    // Two-frequency Lissajous orbit so paths never close exactly.
    let orbit = vec2<f32>(
        sin(t * spd + ph) + 0.35 * sin(t * spd * 1.71 + ph * 0.6),
        cos(t * spd * 0.83 + ph * 1.3) + 0.35 * cos(t * spd * 1.43 + ph * 0.9)
    ) * (rad * 0.75);

    // Low-frequency noise drift — incommensurate per seed.
    let drift = vec2<f32>(
        vnoise(t * 0.30 + fi)        * 2.0 - 1.0,
        vnoise(t * 0.27 + fi + 13.7) * 2.0 - 1.0
    ) * 0.10;

    // Smooth beat wobble: a tiny continuous sinusoidal nudge whose amplitude
    // is the decaying pulse envelope. No discrete jitter, no random snap.
    let wob_dir = vec2<f32>(
        sin(fi * 2.137 + t * 1.7),
        cos(fi * 1.913 + t * 1.3)
    );
    let wob = wob_dir * wobble * (0.020 + 0.025 * hash1(fi + 9.0));

    return base + orbit + drift + wob;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let asp = globals.resolution.x / globals.resolution.y;

    // Centered, aspect-corrected coords (matches preview).
    var p = uv * 2.0 - 1.0;
    p.x = p.x * asp;

    // Per-scene continuous trajectory selector — different orbits per switch
    // without changing during a scene.
    let traj = param(0u) + param(1u) * 0.5;

    // Smooth audio-driven modulators — all continuous, no thresholds.
    let bass = band(0u) * 0.6 + band(1u) * 0.4;
    let pulse = globals.pulse;
    // wobble: smooth combination of pulse + bass; both are continuous signals.
    let wobble = pulse * (0.6 + 0.8 * bass);

    // Highlight cell anchors — also continuous orbits.
    let hp1 = vec2<f32>(sin(t * 0.31 + traj) * 0.7 * asp,
                        cos(t * 0.27 + traj * 0.7) * 0.6);
    let hp2 = vec2<f32>(cos(t * 0.19 + 1.7 - traj) * 0.8 * asp,
                        sin(t * 0.23 + 0.4 + traj * 1.3) * 0.55);

    var f1: f32 = 1e9;
    var f2: f32 = 1e9;
    var idx1: u32 = 0u;

    var best_hp1: f32 = 1e9;
    var idx_hp1: u32 = 0u;
    var best_hp2: f32 = 1e9;
    var idx_hp2: u32 = 0u;

    for (var i: u32 = 0u; i < NUM_POINTS; i = i + 1u) {
        let sp = seed_pos(i, t, wobble, traj);

        let d = distance(p, sp);
        if (d < f1) {
            f2 = f1;
            f1 = d;
            idx1 = i;
        } else if (d < f2) {
            f2 = d;
        }

        let d1 = distance(hp1, sp);
        if (d1 < best_hp1) {
            best_hp1 = d1;
            idx_hp1 = i;
        }
        let d2 = distance(hp2, sp);
        if (d2 < best_hp2) {
            best_hp2 = d2;
            idx_hp2 = i;
        }
    }

    let edge = f2 - f1;

    // Thickness modulated continuously by pulse — no jumps.
    let thickness = 0.0055 + 0.0035 * pulse + 0.0015 * bass;
    let aa = 0.0028;
    let line = 1.0 - smoothstep(thickness, thickness + aa, edge);

    // Vignette in unstretched space.
    var cuv = p;
    cuv.x = cuv.x / asp;
    var vign = smoothstep(1.15, 0.15, length(cuv));
    vign = mix(0.35, 1.0, vign);

    let white = vec3<f32>(1.0, 1.0, 1.0);
    let magenta = vec3<f32>(1.0, 0.18, 0.85);

    let accent = (idx1 == idx_hp1) || (idx1 == idx_hp2);
    var col = white;
    if (accent) {
        col = magenta;
    }

    let bright = 0.7 + 0.5 * globals.level + 0.6 * pulse;
    col = col * (line * vign * bright);

    return vec4<f32>(col, 1.0);
}
