#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub sa: [f32; 3],
    pub sb: [f32; 3],
    pub ra: [f32; 3],
    pub rb: [f32; 3],
}

impl Palette {
    pub fn sa4(&self) -> [f32; 4] { [self.sa[0], self.sa[1], self.sa[2], 1.0] }
    pub fn sb4(&self) -> [f32; 4] { [self.sb[0], self.sb[1], self.sb[2], 1.0] }
    pub fn ra4(&self) -> [f32; 4] { [self.ra[0], self.ra[1], self.ra[2], 1.0] }
    pub fn rb4(&self) -> [f32; 4] { [self.rb[0], self.rb[1], self.rb[2], 1.0] }
}

// Pre-computed float values from u8 RGB (f = u8 / 255.0):
pub static PALETTES: &[Palette] = &[
    // Acid: sa=(0,255,220) sb=(200,0,255) ra=(255,60,0) rb=(0,180,255)
    Palette { sa: [0.0, 1.0, 0.863], sb: [0.784, 0.0, 1.0], ra: [1.0, 0.235, 0.0], rb: [0.0, 0.706, 1.0] },
    // Industrial: sa=(255,120,0) sb=(0,80,255) ra=(255,200,0) rb=(0,200,200)
    Palette { sa: [1.0, 0.471, 0.0], sb: [0.0, 0.314, 1.0], ra: [1.0, 0.784, 0.0], rb: [0.0, 0.784, 0.784] },
    // Rave: sa=(0,255,60) sb=(255,0,120) ra=(200,255,0) rb=(255,80,200)
    Palette { sa: [0.0, 1.0, 0.235], sb: [1.0, 0.0, 0.471], ra: [0.784, 1.0, 0.0], rb: [1.0, 0.314, 0.784] },
    // UV: sa=(180,0,255) sb=(0,80,255) ra=(100,0,200) rb=(60,200,255)
    Palette { sa: [0.706, 0.0, 1.0], sb: [0.0, 0.314, 1.0], ra: [0.392, 0.0, 0.784], rb: [0.235, 0.784, 1.0] },
    // Ember: sa=(255,40,0) sb=(255,160,0) ra=(200,0,80) rb=(255,220,80)
    Palette { sa: [1.0, 0.157, 0.0], sb: [1.0, 0.627, 0.0], ra: [0.784, 0.0, 0.314], rb: [1.0, 0.863, 0.314] },
    // Arctic: sa=(0,200,255) sb=(180,240,255) ra=(0,100,200) rb=(200,255,255)
    Palette { sa: [0.0, 0.784, 1.0], sb: [0.706, 0.941, 1.0], ra: [0.0, 0.392, 0.784], rb: [0.784, 1.0, 1.0] },
    // Poison: sa=(160,255,0) sb=(120,0,255) ra=(255,255,0) rb=(200,80,255)
    Palette { sa: [0.627, 1.0, 0.0], sb: [0.471, 0.0, 1.0], ra: [1.0, 1.0, 0.0], rb: [0.784, 0.314, 1.0] },
];

pub fn palette(index: usize) -> Palette {
    PALETTES[index % PALETTES.len()]
}

pub fn palette_count() -> usize {
    PALETTES.len()
}
