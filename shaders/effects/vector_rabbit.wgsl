// vector_rabbit · Hand-traced 60-segment low-poly geometric rabbit
// drawn as thin white vector lines on black via per-pixel SDF distance to
// the nearest segment. Vertex jitter is hashed by POSITION (not segment
// index) so shared vertices stay glued together. Audio drives jitter
// amplitude, breathing scale, rotation swing, beat glitch tears, and tint.
//
// Engine params:
//   0  jitter      0..1   default 0.25  (× 0.05 base + bands × 0.20)
//   1  rot_swing   0..1   default 0.25  (× 0.40 max amplitude)
//   2  breathe     0..1   default 0.30  (× 0.20 scale-pulse range)
//   3  glitch_amt  0..1   default 0.40  (beat tear strength)
//   4  thickness   0..1   default 0.35  (line half-width)
//   5  tint        0..1   default 0.40  (cyan ←→ magenta accent on beat)

const PI: f32 = 3.14159265359;
const SEG_COUNT: i32 = 60;

const SEGS = array<vec4<f32>, 60>(
    // Right outline + head
    vec4<f32>( 0.18, 0.95,  0.30, 0.85),
    vec4<f32>( 0.30, 0.85,  0.32, 0.45),
    vec4<f32>( 0.32, 0.45,  0.42, 0.30),
    vec4<f32>( 0.42, 0.30,  0.55, 0.10),
    vec4<f32>( 0.55, 0.10,  0.50,-0.02),
    vec4<f32>( 0.50,-0.02,  0.40,-0.05),
    vec4<f32>( 0.40,-0.05,  0.30,-0.10),
    vec4<f32>( 0.30,-0.10,  0.32,-0.30),
    vec4<f32>( 0.32,-0.30,  0.28,-0.55),
    vec4<f32>( 0.28,-0.55,  0.20,-0.78),
    vec4<f32>( 0.20,-0.78,  0.30,-0.92),
    vec4<f32>( 0.30,-0.92,  0.10,-0.95),
    vec4<f32>( 0.10,-0.95, -0.05,-0.92),
    vec4<f32>(-0.05,-0.92, -0.25,-0.95),
    vec4<f32>(-0.25,-0.95, -0.45,-0.85),
    vec4<f32>(-0.45,-0.85, -0.55,-0.65),
    vec4<f32>(-0.55,-0.65, -0.60,-0.40),
    vec4<f32>(-0.60,-0.40, -0.55,-0.10),
    vec4<f32>(-0.55,-0.10, -0.45, 0.10),
    vec4<f32>(-0.45, 0.10, -0.30, 0.25),
    vec4<f32>(-0.30, 0.25, -0.18, 0.40),
    vec4<f32>(-0.18, 0.40, -0.20, 0.55),
    // Left ear
    vec4<f32>(-0.20, 0.55, -0.18, 0.78),
    vec4<f32>(-0.18, 0.78, -0.10, 0.95),
    vec4<f32>(-0.10, 0.95, -0.02, 0.85),
    vec4<f32>(-0.02, 0.85, -0.05, 0.50),
    // Between ears
    vec4<f32>(-0.05, 0.50,  0.10, 0.50),
    vec4<f32>( 0.10, 0.50,  0.18, 0.95),
    // Eye
    vec4<f32>( 0.32, 0.20,  0.40, 0.18),
    vec4<f32>( 0.40, 0.18,  0.38, 0.10),
    vec4<f32>( 0.38, 0.10,  0.32, 0.20),
    // Forehead triangulation
    vec4<f32>( 0.10, 0.50,  0.30, 0.30),
    vec4<f32>( 0.30, 0.30,  0.42, 0.30),
    vec4<f32>( 0.10, 0.50,  0.20, 0.20),
    vec4<f32>( 0.20, 0.20,  0.32, 0.20),
    vec4<f32>( 0.20, 0.20,  0.38, 0.10),
    vec4<f32>( 0.20, 0.20,  0.30,-0.10),
    // Chest / body fan
    vec4<f32>( 0.30,-0.10,  0.10,-0.20),
    vec4<f32>( 0.10,-0.20,  0.32,-0.30),
    vec4<f32>( 0.10,-0.20,  0.20,-0.55),
    vec4<f32>( 0.10,-0.20, -0.10,-0.20),
    vec4<f32>(-0.10,-0.20,  0.20,-0.55),
    vec4<f32>(-0.10,-0.20, -0.05,-0.92),
    vec4<f32>(-0.10,-0.20, -0.30,-0.40),
    // Haunch triangulation
    vec4<f32>(-0.30,-0.40, -0.45,-0.85),
    vec4<f32>(-0.30,-0.40, -0.55,-0.65),
    vec4<f32>(-0.30,-0.40, -0.60,-0.40),
    vec4<f32>(-0.30,-0.40, -0.55,-0.10),
    vec4<f32>(-0.30,-0.40, -0.10,-0.20),
    // Neck / back
    vec4<f32>(-0.30, 0.25, -0.55,-0.10),
    vec4<f32>(-0.30, 0.25, -0.10,-0.20),
    vec4<f32>(-0.18, 0.40,  0.10, 0.50),
    // Ear interiors
    vec4<f32>( 0.18, 0.95,  0.32, 0.45),
    vec4<f32>(-0.10, 0.95, -0.20, 0.55),
    vec4<f32>( 0.22, 0.85,  0.30, 0.55),
    vec4<f32>(-0.13, 0.85, -0.18, 0.55),
    // Belly under
    vec4<f32>( 0.20,-0.55, -0.05,-0.92),
    vec4<f32>( 0.20,-0.55, -0.30,-0.40),
    // Front leg
    vec4<f32>( 0.32,-0.30,  0.28,-0.55),
    vec4<f32>( 0.30,-0.92,  0.20,-0.55),
);

fn h11(n: f32) -> f32 { return fract(sin(n) * 43758.5453123); }

// Distance from p to segment a→b.
fn sd_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1.0e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// Hash a vertex by its (model-space) position so vertices shared between
// segments produce the SAME jittered offset — which keeps the lines glued.
fn jitter_vert(v: vec2<f32>, amp: f32, t: f32) -> vec2<f32> {
    let h1 = h11(v.x * 311.7 + v.y * 127.1);
    let h2 = h11(v.x * 731.5 + v.y * 451.3 + 7.0);
    var jit = vec2<f32>(h1, h2) - vec2<f32>(0.5);
    // Slow per-vertex drift over time so the jitter feels alive.
    jit = jit + vec2<f32>(sin(t * 1.7 + h1 * 6.28),
                          cos(t * 1.3 + h2 * 6.28)) * 0.20;
    return v + jit * amp;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();
    let t   = globals.time;

    // Tunables
    let jit_amt    = 0.005 + param(0u) * 0.05;
    let rot_swing  = param(1u) * 0.40;
    let breathe_a  = param(2u) * 0.20;
    let glitch_k   = 0.5 + param(3u) * 1.5;
    let line_hw    = 0.0020 + param(4u) * 0.0040;
    let tint_amt   = param(5u);

    // Screen → model space (centred, y-up, fits unit-square)
    var p = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0);
    p = p / 0.95;

    // Global rotation swing on mids
    let rot = sin(t * 0.6) * rot_swing * (0.6 + band(3u));
    let cs = cos(rot); let sn = sin(rot);
    p = mat2x2<f32>(cs, -sn, sn, cs) * p;

    // Breathing scale: shrink slightly on bass kick → bigger on decay
    let breathe = 1.0 + breathe_a * sin(t * 1.4) - 0.5 * breathe_a * pls;
    p = p / breathe;

    // Beat glitch — translate horizontal slices on strong pulse
    if (pls > 0.55) {
        let slice = floor(p.y * 14.0);
        let gh = h11(slice * 11.7 + floor(t * 4.0));
        if (gh > 0.65) {
            p.x = p.x + (gh - 0.65) * glitch_k * 0.6;
        }
    }

    // Audio-driven jitter amplitude
    let bass = band(0u) + band(2u);
    let amp = jit_amt + 0.08 * bass + 0.10 * pls;

    // Distance to nearest jittered segment
    var minD: f32 = 1.0e9;
    for (var i: i32 = 0; i < SEG_COUNT; i = i + 1) {
        let sg = SEGS[i];
        let a = jitter_vert(sg.xy, amp, t);
        let b = jitter_vert(sg.zw, amp, t);
        let d = sd_seg(p, a, b);
        if (d < minD) { minD = d; }
    }

    // AA line
    let fw = fwidth(minD) + 1.0e-5;
    let line = 1.0 - smoothstep(line_hw, line_hw + fw * 1.4, minD);

    // Gentle bloom
    let glow = exp(-minD * 180.0) * 0.40;

    let cyan = vec3<f32>(0.55, 0.92, 1.00);
    let mag  = vec3<f32>(1.00, 0.30, 0.85);
    let accent = mix(cyan, mag, tint_amt);

    var col = vec3<f32>(line);
    col = col + accent * glow * (0.6 + 0.8 * pls);
    col = col + accent * line * pls * 0.3;

    // Vignette
    let vd = length(uv - vec2<f32>(0.5));
    col = col * smoothstep(0.95, 0.20, vd);

    col = pow(clamp(col, vec3<f32>(0.0), vec3<f32>(2.0)), vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
