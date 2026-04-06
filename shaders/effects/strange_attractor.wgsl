// StrangeAttractor: fragment-shader Clifford attractor density field.
// Re-implements the gl.POINTS WebGL2 preview by accumulating Gaussian splats
// from many short jittered trajectories per pixel.
// (globals.wgsl prepended: GlobalUniforms, EffectParams, band(), param(), hash() etc.)

const PI: f32 = 3.14159265359;

fn clifford(p: vec2<f32>, P: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(
        sin(P.x * p.y) + P.z * cos(P.x * p.x),
        sin(P.y * p.x) + P.w * cos(P.y * p.y)
    );
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed;
    let aspect = globals.resolution.x / globals.resolution.y;

    // Aspect-corrected centered coords. We map the attractor box [-1.25, 1.25]
    // to ~80% of the shorter screen axis.
    var p = (uv - 0.5) * 2.0;
    if (aspect > 1.0) {
        p.x *= aspect;
    } else {
        p.y /= aspect;
    }
    // Scale so the attractor fills ~80% of viewport.
    let view_scale = 1.25 / 0.8;
    let pix = p * view_scale;

    // ---- Continuous parameter drift (no step jumps). ----
    // Base params from preview: a=-1.7, b=1.8, c=-0.9, d=-0.4
    // Per-scene randomization layered on via param(0..3).
    let seed_off = fx.seed_pad.x;
    var a = -1.7 + 0.35 * sin(t * 0.071 + 1.3 + seed_off * 6.28) + (param(0u) - 0.5) * 0.4;
    var b =  1.8 + 0.30 * cos(t * 0.083 + 0.7 + seed_off * 3.11) + (param(1u) - 0.5) * 0.4;
    var c = -0.9 + 0.25 * sin(t * 0.061 + 2.1 + seed_off * 4.77) + (param(2u) - 0.5) * 0.3;
    var d = -0.4 + 0.25 * cos(t * 0.097 + 0.4 + seed_off * 2.55) + (param(3u) - 0.5) * 0.3;

    // Bass-modulated c for audio reactivity.
    let bass = (band(0u) + band(1u)) * 0.5;
    c = c + (bass - 0.5) * 0.6;

    let P = vec4<f32>(a, b, c, d);

    // Slow rotation.
    let rot = t * 0.05 + sin(t * 0.037) * 0.4;
    let cr = cos(rot);
    let sr = sin(rot);
    // Rotate the *sample* pixel so the attractor appears rotated.
    let pix_r = vec2<f32>(cr * pix.x + sr * pix.y, -sr * pix.x + cr * pix.y);

    // Splat width controlled by pixel size — keep splats ~1.5px equivalent.
    let px_size = 2.0 / globals.resolution.y * view_scale;
    let splat_r = px_size * 1.6;
    let k = 1.0 / (splat_r * splat_r);

    // ---- Density accumulation ----
    // 24 seeds × 60 steps = 1440 ops/pixel.
    let SEEDS: u32 = 24u;
    let STEPS: u32 = 60u;

    var density: f32 = 0.0;

    for (var s: u32 = 0u; s < SEEDS; s = s + 1u) {
        let fs = f32(s);
        // Hash-jittered seed in [-1.4, 1.4].
        let h1 = hash(vec2<f32>(fs * 1.7 + seed_off * 91.3, 3.1));
        let h2 = hash(vec2<f32>(fs * 0.93 + 7.0, fs * 2.17 + seed_off * 13.7));
        var z = (vec2<f32>(h1, h2) * 2.0 - 1.0) * 1.4;

        // Discard a few "burn-in" iterations so we land on the attractor.
        for (var w: u32 = 0u; w < 8u; w = w + 1u) {
            z = clifford(z, P);
        }

        for (var i: u32 = 0u; i < STEPS; i = i + 1u) {
            z = clifford(z, P);
            let dxy = z - pix_r;
            let d2 = dot(dxy, dxy);
            density = density + exp(-k * d2);
        }
    }

    // Normalize roughly.
    let dens = density * (1.0 / f32(SEEDS * STEPS)) * 6.0;

    // Tonemap density → brightness.
    let bright = 1.0 - exp(-dens * 8.0);

    // ---- Cyan / white / magenta wash from preview. ----
    let cyan    = vec3<f32>(0.55, 0.95, 1.10);
    let white   = vec3<f32>(1.00, 1.00, 1.00);
    let magenta = vec3<f32>(1.10, 0.45, 0.95);

    let ct = fract(uv.x * 0.6 + uv.y * 0.4 + t * 0.05);
    var col: vec3<f32>;
    if (ct < 0.5) {
        col = mix(cyan, white, ct * 2.0);
    } else {
        col = mix(white, magenta, (ct - 0.5) * 2.0);
    }

    // Hot core punch — bright spots blow toward white.
    let core = smoothstep(0.55, 1.0, bright);
    col = mix(col, vec3<f32>(1.1, 1.05, 1.15), core * 0.7);

    let level_boost = 0.85 + globals.level * 0.5 + smooth_pulse() * 0.3;
    let rgb = col * bright * level_boost;

    return vec4<f32>(rgb, 1.0);
}
