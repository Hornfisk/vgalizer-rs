//! Per-effect parameter definitions.
//!
//! Each effect gets up to 6 named, range-bounded floats. The order in the
//! returned slice maps directly onto `EffectUniforms.params[0..N]`, so
//! `effect_params("foo")[2]` corresponds to `param(2u)` inside the WGSL.
//!
//! Effects without an entry use the empty slice and behave exactly as
//! before (random params on scene switch in `app.rs`).

#[derive(Debug, Clone, Copy)]
pub struct ParamDef {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: f32,
}

const fn p(name: &'static str, min: f32, max: f32, default: f32, step: f32) -> ParamDef {
    ParamDef { name, min, max, default, step }
}

// === Definitions per effect ===
//
// Values are in the [0..1] domain that the WGSL `param(N)` indexer reads.
// The shader maps that to the actual physical range (e.g. param(0u)*8.0).
// Defaults below are tuned to the "looks good with no tweaking" point.

// === v1 effects ===

const HYPERSPACE: &[ParamDef] = &[
    p("speed",      0.0, 1.0, 0.5, 0.05),  // 0.5..1.5x time multiplier
];

const KALEIDO: &[ParamDef] = &[
    p("spokes",     0.0, 1.0, 0.5, 0.05),  // 3..4
    p("rings",      0.0, 1.0, 0.5, 0.05),  // 3..4
    p("rot_speed",  0.0, 1.0, 0.5, 0.05),  // 0.3..0.8
];

const RING_TUNNEL: &[ParamDef] = &[
    p("rings",      0.0, 1.0, 0.5, 0.05),  // 4..5
    p("speed",      0.0, 1.0, 0.5, 0.05),  // 0.4..0.8
    p("shape",      0.0, 1.0, 0.5, 0.05),
    p("rot_speed",  0.0, 1.0, 0.5, 0.05),  // 0..0.5
];

const WARP_GRID: &[ParamDef] = &[
    p("cols",       0.0, 1.0, 0.5, 0.05),  // 6..7
    p("rows",       0.0, 1.0, 0.5, 0.05),  // 6..7
    p("amp",        0.0, 1.0, 0.5, 0.05),  // 0.05..0.13
    p("freq",       0.0, 1.0, 0.5, 0.05),  // 2..5
];

const MORPH_GEO: &[ParamDef] = &[
    p("layers",     0.0, 1.0, 0.5, 0.05),  // 2..3
    p("sides",      0.0, 1.0, 0.5, 0.05),  // 3..4
];

const SPECTRUM_BARS: &[ParamDef] = &[
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("height",     0.0, 1.0, 0.5, 0.05),  // 0.55..0.85
    p("glow",       0.0, 1.0, 0.5, 0.05),  // 0.30..0.70
];

const SPECTRUM_ORBIT: &[ParamDef] = &[
    p("inner_r",    0.0, 1.0, 0.4, 0.05),  // 0.18..0.23
    p("bar_len",    0.0, 1.0, 0.5, 0.05),  // 0.22..0.32
    p("rot_speed",  0.0, 1.0, 0.5, 0.05),  // 0..0.30
];

const SPECTRUM_TERRAIN: &[ParamDef] = &[
    p("height",     0.0, 1.0, 0.5, 0.05),  // 0.45..0.65
    p("glow",       0.0, 1.0, 0.5, 0.05),  // 0.40..0.70
];

const SPECTRUM_WAVE: &[ParamDef] = &[
    p("lines",      0.0, 1.0, 0.5, 0.05),  // 4..5
    p("amp",        0.0, 1.0, 0.5, 0.05),  // 0.08..0.16
    p("freq",       0.0, 1.0, 0.5, 0.05),  // 2..4
];

// === v2 additions ===

const LINE_MOIRE: &[ParamDef] = &[
    p("ang_jitter", 0.0, 1.0, 0.5, 0.05),
    p("spacing",    0.0, 1.0, 0.5, 0.05),
    p("ang_a",      0.0, 1.0, 0.5, 0.05),
    p("ang_b",      0.0, 1.0, 0.5, 0.05),
];

const WIRE_TUNNEL: &[ParamDef] = &[
    p("stripes",    0.0, 1.0, 0.4, 0.05),  // 16..64
    p("twist",      0.0, 1.0, 0.5, 0.05),
    p("dash_freq",  0.0, 1.0, 0.4, 0.05),  // 0.5..4.0
    p("dash_mix",   0.0, 1.0, 0.5, 0.05),
    p("thickness",  0.0, 1.0, 0.4, 0.05),  // 0.012..0.030
    p("roll",       0.0, 1.0, 0.5, 0.05),
    p("warp",       0.0, 1.0, 0.4, 0.05),  // 0.06..0.20
    p("speed",      0.0, 1.0, 0.5, 0.05),  // 1.5..4.0
];

const VORONOI_PULSE: &[ParamDef] = &[
    p("traj_x",     0.0, 1.0, 0.5, 0.05),
    p("traj_y",     0.0, 1.0, 0.5, 0.05),
];

// mandelbrot_zoom: continually zooms into Seahorse Valley. Three knobs:
// tempo = exp-zoom rate, color = palette phase offset, detail = iteration
// cap multiplier (crunchier vs smoother banding).
const MANDELBROT_ZOOM: &[ParamDef] = &[
    p("tempo",      0.0, 1.0, 0.33, 0.05),  // → 0.04..0.28 zoom rate
    p("color",      0.0, 1.0, 0.0,  0.05),  // palette phase offset
    p("detail",     0.0, 1.0, 0.5,  0.05),  // iter cap × 0.6..1.8
];

// === v3 additions ===

const VECTOR_TERRAIN: &[ParamDef] = &[
    p("horizon",    0.0, 1.0, 0.4, 0.05),  // 0.50..0.80
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("height",     0.0, 1.0, 0.5, 0.05),  // 0.45..1.55
    p("scroll",     0.0, 1.0, 0.5, 0.05),  // 0.4..2.0
    p("wave_amt",   0.0, 1.0, 0.4, 0.05),  // 0..0.40
    p("beat_kick",  0.0, 1.0, 0.5, 0.05),  // 0.5..1.2
];

const LASER_BURST: &[ParamDef] = &[
    p("strip_h",    0.0, 1.0, 0.4, 0.05),
    p("ray_thick",  0.0, 1.0, 0.3, 0.05),
    p("ray_gain",   0.0, 1.0, 0.5, 0.05),
    p("beat_flash", 0.0, 1.0, 0.5, 0.05),
    p("hue",        0.0, 1.0, 0.0, 0.05),
    p("strip_glow", 0.0, 1.0, 0.5, 0.05),
];

const SCOPE_XY: &[ParamDef] = &[
    p("ratio_a",    0.0, 1.0, 0.3, 0.05),
    p("ratio_b",    0.0, 1.0, 0.5, 0.05),
    p("amp",        0.0, 1.0, 0.5, 0.05),
    p("halo",       0.0, 1.0, 0.3, 0.05),
    p("hue",        0.0, 1.0, 0.0, 0.05),
    p("glow",       0.0, 1.0, 0.5, 0.05),
];

const WAVE_DUNES: &[ParamDef] = &[
    p("amp",        0.0, 1.0, 0.4, 0.05),
    p("horizon_y",  0.0, 1.0, 0.4, 0.05),
    p("perspect",   0.0, 1.0, 0.5, 0.05),
    p("bottom_y",   0.0, 1.0, 0.4, 0.05),
    p("row_thick",  0.0, 1.0, 0.3, 0.05),
    p("beat_amp",   0.0, 1.0, 0.5, 0.05),
];

const RADIAL_EQ: &[ParamDef] = &[
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("inner_r",    0.0, 1.0, 0.4, 0.05),
    p("max_len",    0.0, 1.0, 0.5, 0.05),
    p("thickness",  0.0, 1.0, 0.3, 0.05),
    p("hue",        0.0, 1.0, 0.5, 0.05),
    p("ring_glow",  0.0, 1.0, 0.5, 0.05),
];

const HARMONOGRAPH: &[ParamDef] = &[
    p("freq_a",     0.0, 1.0, 0.3, 0.05),
    p("freq_b",     0.0, 1.0, 0.4, 0.05),
    p("halo_w",     0.0, 1.0, 0.4, 0.05),
    p("glow",       0.0, 1.0, 0.5, 0.05),
    p("hue",        0.0, 1.0, 0.3, 0.05),
];

const TV_ACID: &[ParamDef] = &[
    p("smiley_r",   0.0, 1.0, 0.4, 0.05),
    p("melt",       0.0, 1.0, 0.5, 0.05),
    p("glitch",     0.0, 1.0, 0.5, 0.05),
    p("scanlines",  0.0, 1.0, 0.4, 0.05),
    p("chroma_ab",  0.0, 1.0, 0.4, 0.05),
    p("grain",      0.0, 1.0, 0.4, 0.05),
];

const KALEIDO_WARP: &[ParamDef] = &[
    p("segments",   0.0, 1.0, 0.4, 0.05),  // 4..16
    p("rot_speed",  0.0, 1.0, 0.5, 0.05),
    p("warp_amt",   0.0, 1.0, 0.4, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("hue",        0.0, 1.0, 0.0, 0.05),
];

const ISOLINE_FIELD: &[ParamDef] = &[
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("spacing",    0.0, 1.0, 0.4, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("hue",        0.0, 1.0, 0.0, 0.05),
];

const MOEBIUS_GRID: &[ParamDef] = &[
    p("density",    0.0, 1.0, 0.5, 0.05),
    p("pole_max",   0.0, 1.0, 0.4, 0.05),
    p("drift",      0.0, 1.0, 0.4, 0.05),
    p("rot_speed",  0.0, 1.0, 0.5, 0.05),
    p("hue",        0.0, 1.0, 0.0, 0.05),
];

const VECTOR_RABBIT: &[ParamDef] = &[
    p("jitter",     0.0, 1.0, 0.25, 0.05),
    p("rot_swing",  0.0, 1.0, 0.25, 0.05),
    p("breathe",    0.0, 1.0, 0.30, 0.05),
    p("glitch",     0.0, 1.0, 0.40, 0.05),
    p("thickness",  0.0, 1.0, 0.35, 0.05),
    p("tint",       0.0, 1.0, 0.40, 0.05),
];

const CYMATICS: &[ParamDef] = &[
    p("plate_zoom", 0.0, 1.0, 0.4, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("(unused)",   0.0, 1.0, 0.5, 0.05),
    p("hue_phase",  0.0, 1.0, 0.0, 0.05),
    p("grain",      0.0, 1.0, 0.4, 0.05),
];

pub fn effect_params(effect: &str) -> &'static [ParamDef] {
    match effect {
        // v1
        "hyperspace"        => HYPERSPACE,
        "kaleido"           => KALEIDO,
        "ring_tunnel"       => RING_TUNNEL,
        "warp_grid"         => WARP_GRID,
        "morph_geo"         => MORPH_GEO,
        "spectrum_bars"     => SPECTRUM_BARS,
        "spectrum_orbit"    => SPECTRUM_ORBIT,
        "spectrum_terrain"  => SPECTRUM_TERRAIN,
        "spectrum_wave"     => SPECTRUM_WAVE,
        // v2
        "line_moire"        => LINE_MOIRE,
        "wire_tunnel"       => WIRE_TUNNEL,
        "voronoi_pulse"     => VORONOI_PULSE,
        "mandelbrot_zoom"   => MANDELBROT_ZOOM,
        // v3
        "vector_terrain"    => VECTOR_TERRAIN,
        "laser_burst"       => LASER_BURST,
        "scope_xy"          => SCOPE_XY,
        "wave_dunes"        => WAVE_DUNES,
        "radial_eq"         => RADIAL_EQ,
        "harmonograph"      => HARMONOGRAPH,
        "tv_acid"           => TV_ACID,
        "kaleido_warp"      => KALEIDO_WARP,
        "isoline_field"     => ISOLINE_FIELD,
        "moebius_grid"      => MOEBIUS_GRID,
        "cymatics"          => CYMATICS,
        "vector_rabbit"     => VECTOR_RABBIT,
        _ => &[],
    }
}

/// Build an `EffectUniforms` for the given effect, sourcing each parameter
/// from the config map (clamped to its declared range) and falling back to
/// the per-param `default` when missing.
pub fn effect_uniforms_from_config(
    effect: &str,
    fx_params: &std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
) -> crate::gpu::EffectUniforms {
    let defs = effect_params(effect);
    let mut params = [0.0f32; 16];
    let effect_map = fx_params.get(effect);

    for (i, def) in defs.iter().enumerate() {
        if i >= 16 { break; }
        let v = effect_map
            .and_then(|m| m.get(def.name))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(def.default)
            .clamp(def.min, def.max);
        params[i] = v;
    }

    crate::gpu::EffectUniforms {
        params,
        seed: 0.0,
        _pad: [0.0; 3],
    }
}
