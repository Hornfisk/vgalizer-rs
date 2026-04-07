// tv_acid · CRT calibration test pattern (SMPTE bars + grayscale + hatching)
// with a melting acid-yellow smiley in the centre, glitch tears, chromatic
// aberration, scanlines, aperture grille tint, grain.

const PI: f32 = 3.14159265359;

fn h1(n: f32) -> f32 { return fract(sin(n) * 43758.5453123); }

fn smpte_bar(u: f32) -> vec3<f32> {
    let i = i32(floor(u * 7.0));
    if (i == 0) { return vec3<f32>(0.75, 0.75, 0.75); }
    if (i == 1) { return vec3<f32>(0.75, 0.75, 0.00); }
    if (i == 2) { return vec3<f32>(0.00, 0.75, 0.75); }
    if (i == 3) { return vec3<f32>(0.00, 0.75, 0.00); }
    if (i == 4) { return vec3<f32>(0.75, 0.00, 0.75); }
    if (i == 5) { return vec3<f32>(0.75, 0.00, 0.00); }
    return vec3<f32>(0.00, 0.00, 0.75);
}

fn test_pattern(uv_in: vec2<f32>) -> vec3<f32> {
    let y = 1.0 - uv_in.y;
    var c: vec3<f32>;
    if (y < 0.55) {
        c = smpte_bar(uv_in.x);
    } else if (y < 0.66) {
        let i = i32(floor(uv_in.x * 7.0));
        if      (i == 0) { c = vec3<f32>(0.0, 0.0, 0.75); }
        else if (i == 1) { c = vec3<f32>(0.05); }
        else if (i == 2) { c = vec3<f32>(0.75, 0.0, 0.75); }
        else if (i == 3) { c = vec3<f32>(0.05); }
        else if (i == 4) { c = vec3<f32>(0.0, 0.75, 0.75); }
        else if (i == 5) { c = vec3<f32>(0.05); }
        else             { c = vec3<f32>(0.75); }
    } else {
        let g = floor(uv_in.x * 8.0) / 7.0;
        c = vec3<f32>(g);
        if (uv_in.x > 0.50 && uv_in.x < 0.62) { c = vec3<f32>(0.0); }
        if (uv_in.x > 0.78 && uv_in.x < 0.86) { c = vec3<f32>(0.02); }
    }
    let g2 = fract(uv_in * vec2<f32>(32.0, 18.0));
    let hatch = step(0.96, g2.x) + step(0.96, g2.y);
    c = mix(c, vec3<f32>(0.0), clamp(hatch, 0.0, 1.0) * 0.35);
    return c;
}

// Distorted face SDF: cheek bulges, bottom sags more than top.
fn face_sdf(p: vec2<f32>, r: f32, melt: f32, t: f32) -> f32 {
    // Per-x vertical sag — bottom of the face droops in fingers.
    let sag = 0.18 * melt * (0.5 + 0.5 * sin(p.x * 7.0 + t * 0.6));
    let lower = step(0.0, -p.y); // 1 if below face centre
    let q = vec2<f32>(p.x, p.y + sag * lower);
    // Cheek bulge — slight x stretch on bass
    let stretched = vec2<f32>(q.x * (1.0 - 0.06 * melt), q.y);
    return length(stretched) - r;
}

fn smiley_mask(p_in: vec2<f32>, r: f32, melt: f32, t: f32) -> f32 {
    let face = face_sdf(p_in, r, melt, t);

    // Asymmetric eye drift: left eye sags faster, both wobble slightly.
    let l_drop = melt * (0.06 + 0.04 * sin(t * 1.7));
    let r_drop = melt * (0.03 + 0.04 * sin(t * 2.1 + 1.3));
    let l_x_jit = 0.02 * melt * sin(t * 1.1);
    let r_x_jit = 0.02 * melt * sin(t * 1.4 + 2.0);
    let leye = p_in
        - vec2<f32>(-0.32 * r, 0.30 * r)
        + vec2<f32>(l_x_jit, l_drop);
    let reye = p_in
        - vec2<f32>( 0.32 * r, 0.30 * r)
        + vec2<f32>(r_x_jit, r_drop);
    // Eyes squash vertically as they melt → look like winking
    let eye_r = r * 0.10;
    let l_squash = 1.0 + 0.6 * melt * sin(t * 1.9);
    let r_squash = 1.0 + 0.6 * melt * sin(t * 2.3 + 0.7);
    let l_e = length(vec2<f32>(leye.x, leye.y * l_squash)) - eye_r;
    let r_e = length(vec2<f32>(reye.x, reye.y * r_squash)) - eye_r;

    // Mouth: arc that warps along its length and tilts with bass.
    let mouth_drop = melt * 0.13 + 0.02 * sin(t * 0.9);
    let tilt = 0.12 * melt * sin(t * 0.7);
    let mp_raw = p_in - vec2<f32>(0.0, -0.18 * r);
    // Rotate by tilt
    let cs = cos(tilt); let sn = sin(tilt);
    let mp = vec2<f32>(cs * mp_raw.x - sn * (mp_raw.y + mouth_drop),
                       sn * mp_raw.x + cs * (mp_raw.y + mouth_drop));
    let mr = r * (0.55 + 0.10 * melt * sin(t * 1.3));
    let arc_d = abs(length(mp) - mr) - r * (0.05 + 0.04 * melt);
    // Phase distortion along the arc — wavy mouth
    let ang = atan2(mp.y, mp.x);
    let warp = 0.012 * melt * sin(ang * 5.0 + t * 2.0);
    var mc = arc_d + warp;
    if (mp.y > 0.0) { mc = 1.0; }

    let aa = 0.004;
    let face_a = 1.0 - smoothstep(-aa, aa, face);
    let eye_a  = (1.0 - smoothstep(-aa, aa, l_e)) + (1.0 - smoothstep(-aa, aa, r_e));
    let mou_a  = 1.0 - smoothstep(-aa, aa, mc);
    let hole = clamp(eye_a + mou_a, 0.0, 1.0);
    return clamp(face_a - hole, 0.0, 1.0);
}

// Vertical "icicle" drips hanging below the face: a small set of warped
// columns whose lengths grow with melt.
fn drip_mask(p: vec2<f32>, r: f32, melt: f32, t: f32) -> f32 {
    if (p.y > -r * 0.5) { return 0.0; }
    var m: f32 = 0.0;
    for (var i: i32 = 0; i < 6; i = i + 1) {
        let fi = f32(i);
        let cx = (fi - 2.5) * (r * 0.30) + 0.04 * sin(t * 0.7 + fi);
        let len = r * (0.35 + 0.30 * melt + 0.15 * sin(t * 0.9 + fi * 1.7));
        let top = -r * 0.6;
        let bot = top - len;
        let in_y = step(bot, p.y) * step(p.y, top);
        // Width tapers along length
        let frac = clamp((top - p.y) / max(len, 1.0e-3), 0.0, 1.0);
        let width = r * (0.030 + 0.015 * (1.0 - frac));
        let dx = abs(p.x - cx) - width;
        let aa = 0.004;
        let col = (1.0 - smoothstep(-aa, aa, dx)) * in_y;
        m = max(m, col);
    }
    return clamp(m, 0.0, 1.0);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();

    // Tunables
    let smiley_r   = 0.12 + param(0u) * 0.20;
    let melt_extra = param(1u) * 0.30;
    let glitch_k   = 0.5 + param(2u) * 2.0;
    let scan_str   = 0.10 + param(3u) * 0.40;
    let chroma     = 0.001 + param(4u) * 0.008;
    let grain_amt  = param(5u) * 0.10;

    // Glitch tears
    let gt = floor(globals.time * 1.2);
    let band_y = floor(uv.y * 36.0);
    let gh = h1(band_y * 13.7 + gt);
    var tear_amt: f32 = 0.0;
    if (gh > 0.92) { tear_amt = (h1(band_y + gt * 7.0) - 0.5) * 0.18 * glitch_k; }
    if (pls > 0.7 && h1(band_y + 91.0) > 0.7) {
        tear_amt = tear_amt + (h1(band_y + 33.0) - 0.5) * 0.35 * glitch_k;
    }
    var tuv = uv;
    tuv.x = fract(tuv.x + tear_amt);

    let ca = chroma + 0.005 * pls;
    let base_r = test_pattern(vec2<f32>(tuv.x + ca, tuv.y));
    let base_g = test_pattern(tuv);
    let base_b = test_pattern(vec2<f32>(tuv.x - ca, tuv.y));
    var col = vec3<f32>(base_r.r, base_g.g, base_b.b);

    // Smiley
    var p = uv - vec2<f32>(0.5);
    p.x = p.x * aspect;
    p.y = -p.y;
    let bass = band(0u) + band(1u);
    let melt = 0.12 + 0.20 * bass + 0.15 * pls + melt_extra;
    p.x = p.x + tear_amt * 0.6;
    let sp = p;

    let acid = vec3<f32>(1.00, 0.95, 0.05);

    // Drip icicles (drawn first, behind the face)
    let drips = drip_mask(sp, smiley_r, melt, globals.time);
    col = mix(col, acid * 0.85, drips);

    let mask = smiley_mask(sp, smiley_r, melt, globals.time);
    let dC = length(sp);
    let face = mix(acid * 1.05, acid * 0.78, smoothstep(0.0, smiley_r, dC));
    col = mix(col, face, mask);

    // Outline traces the distorted face SDF, not just a circle
    let face_d = face_sdf(sp, smiley_r, melt, globals.time);
    let ring_d = abs(face_d);
    col = mix(col, vec3<f32>(0.0), 1.0 - smoothstep(0.001, 0.004, ring_d));

    // Scanlines
    let scan = 0.5 + 0.5 * sin(uv.y * res.y * PI);
    col = col * (1.0 - scan_str * (1.0 - scan));

    // Aperture grille
    let frag_x = uv.x * res.x;
    let ag = frag_x - floor(frag_x / 3.0) * 3.0;
    var ag_tint: vec3<f32>;
    if (ag < 1.0) { ag_tint = vec3<f32>(1.05, 0.95, 0.95); }
    else if (ag < 2.0) { ag_tint = vec3<f32>(0.95, 1.05, 0.95); }
    else { ag_tint = vec3<f32>(0.95, 0.95, 1.05); }
    col = col * ag_tint;

    // Grain
    let n = h1(uv.x * 397.0 + uv.y * 712.3 + globals.time * 60.0);
    col = col + (n - 0.5) * grain_amt;

    // Vignette
    let vd = length(uv - vec2<f32>(0.5));
    col = col * smoothstep(0.95, 0.25, vd);

    // Beat flash
    col = col + vec3<f32>(0.04, 0.04, 0.06) * pls;

    col = pow(clamp(col, vec3<f32>(0.0), vec3<f32>(1.5)), vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
