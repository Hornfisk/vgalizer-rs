// laser_burst · Central horizontal spectrum strip + radial laser rays.
// 32 thin radial lasers shoot from the centre with brightness = bands.

const PI: f32 = 3.14159265359;
const TAU: f32 = 6.28318530718;
const RAYS: i32 = 48;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let aspect = res.x / res.y;
    let pls = smooth_pulse();

    // Centred coords
    var p = (uv - vec2<f32>(0.5)) * 2.0;
    p.x = p.x * aspect;
    let r = length(p);
    let ang = atan2(p.y, -p.x); // mirrored so x→right matches preview

    // Tunables
    let strip_h     = 0.05 + param(0u) * 0.18;     // central spectrum strip height
    let ray_thick   = 0.0008 + param(1u) * 0.004;  // tangential thickness
    let ray_gain    = 0.6  + param(2u) * 1.4;      // ray brightness
    let beat_flash  = 0.5  + param(3u) * 1.5;
    let hue_shift   = param(4u);                   // 0 green → 0.5 cyan → 1 magenta
    let strip_glow  = 0.5  + param(5u) * 1.5;

    // Accent colour mix
    let green   = vec3<f32>(0.30, 1.00, 0.40);
    let cyan    = vec3<f32>(0.30, 0.95, 1.00);
    let magenta = vec3<f32>(1.00, 0.30, 0.85);
    let accent  = mix(mix(green, cyan, clamp(hue_shift * 2.0, 0.0, 1.0)),
                      magenta,
                      clamp((hue_shift - 0.5) * 2.0, 0.0, 1.0));

    var col = vec3<f32>(0.0);

    // === Central spectrum strip ===
    if (abs(p.y) < strip_h * 1.2) {
        // Map x to band index 0..31
        let bi_f = (p.x / aspect + 1.0) * 0.5 * 32.0;
        let bi = u32(clamp(bi_f, 0.0, 31.0));
        let lvl = band(bi);
        let bar_top = lvl * strip_h;
        let in_bar = step(abs(p.y), bar_top);

        let bar_col = mix(vec3<f32>(0.10, 0.40, 0.10), accent, lvl);
        col = col + bar_col * in_bar * (0.7 + 0.6 * pls);

        // Strip baseline glow
        let baseline = 1.0 - smoothstep(0.0, strip_h * 0.04, abs(p.y));
        col = col + accent * baseline * 0.35 * strip_glow;
    }

    // === Radial laser rays ===
    // Use band(i mod 32) for each ray
    let ang_n = (ang + PI) / TAU * f32(RAYS);
    let ray_idx = i32(floor(ang_n + 0.5));
    let ray_idx_w = ((ray_idx % RAYS) + RAYS) % RAYS;
    let bi2 = u32((ray_idx_w * 32 / RAYS) % 32);
    let lvl_r = band(bi2);

    let ray_centre = (f32(ray_idx) + 0.5) / f32(RAYS) * TAU - PI;
    var d_ang = ang - ray_centre;
    d_ang = atan2(sin(d_ang), cos(d_ang));
    let tang = abs(d_ang) * r;
    let aa = 1.5 / res.y;
    let ray_w = 1.0 - smoothstep(ray_thick * 0.4, ray_thick * 1.6 + aa, tang);

    // Only outside the central strip
    if (r > strip_h * 0.6) {
        let radial_fade = 1.0 - smoothstep(0.0, 1.6, r);
        let beat_boost = 1.0 + pls * beat_flash;
        col = col + accent * ray_w * lvl_r * ray_gain * radial_fade * beat_boost;
        // Hot core
        col = col + vec3<f32>(1.0) * ray_w * pow(lvl_r, 3.0) * radial_fade * 0.35;
    }

    // Beat shockwave ring
    let bt = globals.beat_time;
    let ring_r = bt * 1.2;
    let ring_d = abs(r - ring_r);
    let ring_amp = exp(-bt * 4.0);
    col = col + accent * exp(-ring_d * 80.0) * ring_amp * 0.7;

    // Centre glow
    col = col + accent * exp(-r * r * 25.0) * (0.3 + 0.5 * pls);

    // Vignette
    col = col * smoothstep(1.4, 0.2, r);

    col = 1.0 - exp(-col * 1.4);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
