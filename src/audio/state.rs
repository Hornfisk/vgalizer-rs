use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const N_BANDS: usize = 32;

/// Lock-free bridge between audio callback thread and render thread.
pub struct AtomicAudioState {
    level: AtomicU32,
    bands: [AtomicU32; N_BANDS],
    pub generation: AtomicU64,
}

// SAFETY: AtomicU32/AtomicU64 are Send+Sync
unsafe impl Send for AtomicAudioState {}
unsafe impl Sync for AtomicAudioState {}

impl AtomicAudioState {
    pub fn new() -> Self {
        Self {
            level: AtomicU32::new(0),
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
            generation: AtomicU64::new(0),
        }
    }

    pub fn store_level(&self, v: f32) {
        self.level.store(v.to_bits(), Ordering::Release);
    }

    pub fn load_level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Acquire))
    }

    pub fn store_bands(&self, bands: &[f32; N_BANDS]) {
        for (i, &v) in bands.iter().enumerate() {
            self.bands[i].store(v.to_bits(), Ordering::Release);
        }
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn load_bands(&self) -> [f32; N_BANDS] {
        std::array::from_fn(|i| f32::from_bits(self.bands[i].load(Ordering::Acquire)))
    }
}

impl Default for AtomicAudioState {
    fn default() -> Self { Self::new() }
}
