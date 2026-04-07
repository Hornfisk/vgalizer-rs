// MandelbrotZoom: infinite exponential zoom into a fixed deep point.
// Float precision will eventually break — that's part of the show.
// SceneManager rotates effects every ~30s, so unbounded zoom time is fine.

const PI: f32 = 3.14159265359;

// Techno palette: deep blue -> cool white -> magenta -> black.
fn techno_palette(t_in: f32) -> vec3<f32> {
    let t = fract(t_in);
    let c0 = vec3<f32>(0.02, 0.05, 0.25); // deep blue
    let c1 = vec3<f32>(0.85, 0.95, 1.00); // cool white
    let c2 = vec3<f32>(0.95, 0.10, 0.75); // magenta
    let c3 = vec3<f32>(0.00, 0.00, 0.00); // black
    let seg = t * 4.0;
    if (seg < 1.0) {
        return mix(c0, c1, smoothstep(0.0, 1.0, seg));
    } else if (seg < 2.0) {
        return mix(c1, c2, smoothstep(0.0, 1.0, seg - 1.0));
    } else if (seg < 3.0) {
        return mix(c2, c3, smoothstep(0.0, 1.0, seg - 2.0));
    } else {
        return mix(c3, c0, smoothstep(0.0, 1.0, seg - 3.0));
    }
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // param(0u) = tempo     → 0.04..0.28 exp-zoom rate (default ~0.12)
    // param(1u) = color     → 0..1 palette phase offset
    // param(2u) = detail    → iteration cap multiplier (0.6..1.8)
    let zoom_rate  = 0.04 + param(0u) * 0.24;
    let color_off  = param(1u);
    let detail_mul = 0.6 + param(2u) * 1.2;

    let t = globals.time * globals.fx_speed;

    // Aspect-corrected centered coords in [-asp/2, asp/2] x [-0.5, 0.5].
    let asp = globals.resolution.x / globals.resolution.y;
    let p = (uv - 0.5) * vec2<f32>(asp, 1.0);

    // Fixed deep target — Seahorse Valley.
    let center = vec2<f32>(-0.74364388703, 0.13182590421);

    // Continuous exponential zoom forever.
    let zoom = exp(t * zoom_rate);

    // Sample point in the complex plane.
    let c = center + p * (3.0 / zoom);

    // Iteration cap scales with zoom depth, modulated by the detail knob.
    let base_iter = (180.0 + log(zoom) * 30.0) * detail_mul;
    let max_iter: u32 = u32(clamp(base_iter, 120.0, 600.0));

    var z = vec2<f32>(0.0, 0.0);
    var iter: f32 = 0.0;
    var esc: f32 = 0.0;

    // Hard literal upper bound; break early at max_iter.
    for (var i: u32 = 0u; i < 600u; i = i + 1u) {
        if (i >= max_iter) { break; }
        z = vec2<f32>(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
        let d2 = dot(z, z);
        if (d2 > 256.0) {
            // Smooth escape-time coloring.
            let log_zn = 0.5 * log(d2);
            let nu = log(log_zn / log(2.0)) / log(2.0);
            iter = f32(i) + 1.0 - nu;
            esc = 1.0;
            break;
        }
        iter = f32(i) + 1.0;
    }

    // Inside the set: pure black.
    if (esc < 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Palette cycles with time + a small audio-driven phase nudge + the
    // user's color offset knob.
    let palette_shift = globals.beat * 0.07 + globals.pulse * 0.05 + color_off;
    let tn = iter / f32(max_iter);
    let phase = tn * 4.0 + t * 0.2 + palette_shift;
    var col = techno_palette(phase);

    // Audio-reactive brightness — kept dim enough that the trail post-pass
    // doesn't blow it to white. Floor 0.45 (was 0.85) and a gentler level
    // coefficient 0.25 (was 0.35) so the fractal banding stays readable.
    col = col * (0.45 + 0.25 * globals.level + 0.25 * smooth_pulse());

    // Subtle blue-tinted lift from low-band energy average.
    var band_mix: f32 = 0.0;
    for (var i: u32 = 0u; i < 8u; i = i + 1u) {
        band_mix = band_mix + band(i);
    }
    band_mix = band_mix / 8.0;
    col = col + vec3<f32>(0.0, 0.05, 0.10) * band_mix;

    // Vignette.
    let q = uv - 0.5;
    let vig = smoothstep(0.85, 0.2, length(q));
    col = col * mix(0.6, 1.0, vig);

    return vec4<f32>(col, 1.0);
}
