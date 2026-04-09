use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const N_BANDS: usize = 32;

/// Bridge between audio capture thread and render thread.
///
/// Carries the smoothed `level` (used for visual level/pulse) and the 32
/// log-spaced spectral bands (used by band-reactive effects) via
/// latest-value atomics, which is fine because render just wants the most
/// recent reading per frame.
///
/// The beat tracker's input — low-band spectral flux — is handled
/// differently. Flux is **event-rate critical**: a single audio block
/// (~11.6 ms at 44.1 kHz / 512 frames) may contain a kick transient, and
/// render at 60 Hz polls slower than audio produces samples (~86 Hz).
/// A latest-value atomic silently overwrote ~30 % of flux samples
/// before render could read them, which translated into a systematic
/// ~9 % IOI inflation in the beat tracker (see T6a 9/8 bug, 2026-04-09).
///
/// The fix: push every flux sample from the audio thread into a small
/// SPSC ring buffer with its own wall-clock timestamp, and have the
/// render thread drain the full backlog every frame. This way every
/// block is processed and onset intervals are measured against the
/// audio-thread timestamps (which are the "real" audio event times),
/// not the render-thread sampling phase.
pub struct AtomicAudioState {
    level: AtomicU32,
    bands: [AtomicU32; N_BANDS],
    pub generation: AtomicU64,
    /// Shared reference timestamp. Both audio and render threads take
    /// `start.elapsed()` to get a common wall-clock `t`, so flux sample
    /// timestamps pushed by the audio thread can be compared directly
    /// against the render-thread time used by the rest of the app.
    pub start: Instant,
    /// Ring of (t, flux) samples pushed by the audio thread, drained by
    /// render. Bounded so a stalled render can't balloon memory.
    flux_ring: Mutex<VecDeque<(f64, f32)>>,
}

// SAFETY: all fields are Send+Sync (AtomicU32/U64 + Mutex + Instant).
unsafe impl Send for AtomicAudioState {}
unsafe impl Sync for AtomicAudioState {}

/// Hard cap on ring depth. At 86 Hz audio push rate this is ~3 s of
/// backlog — way more than a healthy render ever falls behind by.
/// If the ring fills we drop the oldest samples, which is the right
/// thing: stale flux samples can't rescue a stalled render and would
/// only confuse the beat tracker's time axis if drained much later.
const FLUX_RING_CAP: usize = 256;

impl AtomicAudioState {
    pub fn new() -> Self {
        Self {
            level: AtomicU32::new(0),
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
            generation: AtomicU64::new(0),
            start: Instant::now(),
            flux_ring: Mutex::new(VecDeque::with_capacity(FLUX_RING_CAP)),
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

    /// Push a (timestamp, flux) sample from the audio thread. `t` is
    /// seconds since `self.start` — the audio thread is expected to
    /// compute this as `self.start.elapsed().as_secs_f64()` right after
    /// finishing the FFT for its current block. Called once per audio
    /// block. If the ring is full, drops the oldest sample.
    pub fn push_flux_sample(&self, t: f64, flux: f32) {
        if let Ok(mut ring) = self.flux_ring.lock() {
            if ring.len() >= FLUX_RING_CAP {
                ring.pop_front();
            }
            ring.push_back((t, flux));
        }
    }

    /// Drain all pending flux samples into `out` in FIFO order. Called
    /// by the render thread once per frame before updating the beat
    /// tracker; the caller is expected to feed each sample to
    /// `BeatTracker::update` in order so the tracker sees a correctly
    /// time-stamped flux stream.
    pub fn drain_flux_samples(&self, out: &mut Vec<(f64, f32)>) {
        out.clear();
        if let Ok(mut ring) = self.flux_ring.lock() {
            out.extend(ring.drain(..));
        }
    }
}

impl Default for AtomicAudioState {
    fn default() -> Self { Self::new() }
}
