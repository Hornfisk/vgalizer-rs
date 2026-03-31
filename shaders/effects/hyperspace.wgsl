// Hyperspace: procedural 3D starfield rushing toward camera.
// Stars are generated procedurally per-pixel — no CPU-side star list needed.

#import globals

struct EffectParams {
    params: array<f32, 16>,
    seed: f32,
    _pad: vec3<f32>,
};
@group(1) @binding(0) var<uniform> fx: EffectParams;

// params[0] = speed multiplier (default 1.0)
// params[1] = star density layers (default 5.0)

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let t = globals.time * globals.fx_speed * (fx.params[0] + 0.5);
    let speed = 0.5 + globals.level * 1.5 + globals.beat * 0.5;
    let n_layers = max(3.0, fx.params[1]);
    let pulse = smooth_pulse();
    let aspect = globals.resolution.x / globals.resolution.y;

    var color = vec3<f32>(0.0);

    // Multiple procedural star layers at different depths
    for (var layer = 0u; layer < 80u; layer++) {
        let lf = f32(layer);
        let seed_val = lf * 1.7 + fx.seed * 100.0;

        // Star 3D position (randomized per layer)
        let sx = fract(hash(vec2<f32>(seed_val, 0.1)) + 0.5) * 2.0 - 1.0;
        let sy = fract(hash(vec2<f32>(seed_val, 0.2)) + 0.5) * 2.0 - 1.0;
        // Depth cycles 0→1 over time at layer-specific speed
        let depth_phase = fract(seed_val * 0.137 + t * speed * (0.3 + fract(seed_val * 0.31) * 0.7));
        let sz = 0.01 + depth_phase;  // z: 0.01 (near) to 1.0 (far)

        // Project: near stars are bigger, further apart from center
        let px = sx / sz;
        let py = sy / sz;

        // Screen-space position (centered)
        let screen_x = (uv.x - 0.5) * aspect * 2.0;
        let screen_y = (uv.y - 0.5) * 2.0;

        let dist = length(vec2<f32>(screen_x - px, screen_y - py));

        // Star size grows as it approaches (smaller sz = closer)
        let star_size = 0.002 / sz;
        let brightness = (1.0 - sz) * smoothstep(star_size * 1.5, 0.0, dist);

        // Color: mix palette colors by layer
        let col_t = fract(seed_val * 0.41);
        let star_color = mix(globals.palette_sa.rgb, globals.palette_sb.rgb, col_t);

        color += star_color * brightness * (1.5 + pulse * 0.5);

        // Streak: motion blur toward center on fast approach
        if sz < 0.3 {
            let streak_len = (0.3 - sz) * speed * 0.15;
            let to_center = normalize(vec2<f32>(-px, -py) + vec2<f32>(0.001));
            let streak_dist = abs(dot(vec2<f32>(screen_x - px, screen_y - py), to_center));
            let perp_dist = length(vec2<f32>(screen_x - px, screen_y - py)) - streak_dist;
            let streak_bright = smoothstep(star_size * 0.5, 0.0, abs(perp_dist))
                              * smoothstep(streak_len, 0.0, streak_dist)
                              * (1.0 - sz / 0.3);
            color += star_color * streak_bright * 0.5;
        }
    }

    // Subtle vignette
    let vignette = 1.0 - length((uv - 0.5) * 1.5);
    color *= max(0.0, vignette);

    return vec4<f32>(color, 1.0);
}
