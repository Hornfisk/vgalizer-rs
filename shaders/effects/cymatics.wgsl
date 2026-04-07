// cymatics · Chladni nodal lines from f = cos(mπx)cos(nπy) - cos(nπx)cos(mπy).
// (m,n) morphs between hashed integer pairs every couple of beats.

const PI: f32 = 3.14159265359;

const MODE_COUNT: i32 = 12;

fn mode_pair(idx: i32) -> vec2<f32> {
    // 12 hand-picked (m,n) pairs with distinct nodal patterns
    let i = ((idx % MODE_COUNT) + MODE_COUNT) % MODE_COUNT;
    if (i == 0)  { return vec2<f32>(3.0, 2.0); }
    if (i == 1)  { return vec2<f32>(4.0, 3.0); }
    if (i == 2)  { return vec2<f32>(5.0, 2.0); }
    if (i == 3)  { return vec2<f32>(5.0, 4.0); }
    if (i == 4)  { return vec2<f32>(6.0, 3.0); }
    if (i == 5)  { return vec2<f32>(6.0, 5.0); }
    if (i == 6)  { return vec2<f32>(7.0, 2.0); }
    if (i == 7)  { return vec2<f32>(7.0, 4.0); }
    if (i == 8)  { return vec2<f32>(8.0, 3.0); }
    if (i == 9)  { return vec2<f32>(8.0, 5.0); }
    if (i == 10) { return vec2<f32>(9.0, 4.0); }
    return vec2<f32>(10.0, 7.0);
}

fn chladni(p: vec2<f32>, m: f32, n: f32) -> f32 {
    return cos(m * PI * p.x) * cos(n * PI * p.y)
         - cos(n * PI * p.x) * cos(m * PI * p.y);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let res = globals.resolution;
    let pls = smooth_pulse();

    // Plate scale: param 0 controls zoom
    let plate_zoom = 1.2 + param(0u) * 1.0;
    let p = (uv - vec2<f32>(0.5)) * 2.0 * vec2<f32>(res.x / res.y, -1.0) * plate_zoom;

    // Mode interpolation: stepped by quarter-beat counter
    let beat_idx = i32(globals.bpm * globals.time / 60.0 * 0.5); // every 2 beats
    let m_morph = fract(globals.bpm * globals.time / 60.0 * 0.5);
    let k = m_morph * m_morph * (3.0 - 2.0 * m_morph);
    let cur = mode_pair(beat_idx);
    let nxt = mode_pair(beat_idx + 1);
    var mn = mix(cur, nxt, k);

    // Audio wobble drives small offsets
    let wob = 0.15 * sin(globals.time * 1.1) + 0.1 * band(2u);
    mn.x = mn.x + wob;
    mn.y = mn.y - wob * 0.5;

    let v = chladni(p, mn.x, mn.y);

    let fw = fwidth(v) + 1.0e-5;
    let nodal_w = 1.0 - smoothstep(0.0, fw * 1.6, abs(v));
    let glow = exp(-v * v * 0.6) * 0.35;

    let cyan = vec3<f32>(0.30, 0.95, 1.00);
    let mag  = vec3<f32>(1.00, 0.30, 0.85);
    let ph = 0.5 + 0.5 * sin(globals.time * 0.13 + param(3u) * 6.28);
    let tint = mix(cyan, mag, ph);

    var col = vec3<f32>(nodal_w);
    col = col + tint * glow * (0.6 + 0.6 * pls);

    // Grain in antinodes
    let grain = fract(sin(dot(uv * res, vec2<f32>(12.9898, 78.233)) + globals.time * 0.5) * 43758.5453);
    col = col + vec3<f32>(grain * (param(4u) * 0.15 + 0.05)) * (1.0 - nodal_w);

    let r = length(p);
    col = col + tint * pls * 0.18 * exp(-r * 1.4);

    // Plate edge vignette
    let plate = smoothstep(1.55, 1.30, max(abs(p.x), abs(p.y)));
    col = col * plate;

    col = 1.0 - exp(-col * 1.5);
    col = pow(col, vec3<f32>(0.92));
    return vec4<f32>(col, 1.0);
}
