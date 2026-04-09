/// Audio analyzer: RMS level + 32 log-spaced frequency bands + low-band
/// spectral flux (for the beat tracker).
///
/// Originally an exact port of Python AudioAnalyzer._cb(). Since the port,
/// it also tracks `kick_flux` — the positive half-wave derivative of the
/// unsmoothed low-band energy — which is what the beat tracker actually
/// consumes. The old full-band RMS path still feeds the visual `level`
/// knob so shaders keep their current feel; the new flux path is
/// separate so transients don't get smeared by the visual smoothing.
///
/// Buffers (`mono_buf`, `fft_buf`, `spec_buf`) are pre-allocated in `new()`
/// and reused across calls so the audio callback (~170 Hz at 256-sample
/// blocks) never hits the allocator. The FFT plan is also cached once.
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;

use super::state::N_BANDS;

const N_FFT: usize = 512;
const PEAK_FLOOR: f32 = 0.001;
const PEAK_DECAY: f32 = 0.9997;

/// Number of low-frequency bands to sum for kick-energy detection. At
/// 44.1 kHz with N_FFT=512 and the (i/N)^0.65 log mapping, bands [0..4]
/// cover roughly low-bass through low-mid (~86–430 Hz) — the part of
/// the spectrum where kick transients live. Widening this range picks
/// up too much bassline sustain; narrowing it misses un-compressed
/// kicks.
const KICK_LOW_BANDS: usize = 4;

pub struct AudioAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    peak: f32,
    band_peaks: [f32; N_BANDS],
    smoothed_level: f32,
    smoothed_bands: [f32; N_BANDS],
    n_out: usize,
    max_bin: usize,

    // Pre-allocated work buffers. Reused across calls so the audio
    // callback never touches the allocator.
    mono_buf: Vec<f32>,
    fft_buf: Vec<Complex<f32>>,
    spec_buf: Vec<f32>,

    /// Previous-block raw kick energy. Used to compute the positive
    /// half-wave flux (onset strength) for the beat tracker.
    prev_kick_energy: f32,
    /// Most-recent un-smoothed kick flux exposed via `kick_flux()`.
    kick_flux: f32,
}

impl AudioAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        let n_out = N_FFT / 2 + 1; // rfft output bins
        let nyquist = sample_rate as f32 * 0.5;
        let max_bin = (n_out as f32 * 14000.0 / nyquist) as usize;
        let max_bin = max_bin.max(2).min(n_out);
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(N_FFT);
        Self {
            fft,
            peak: PEAK_FLOOR,
            band_peaks: [PEAK_FLOOR; N_BANDS],
            smoothed_level: 0.0,
            smoothed_bands: [0.0; N_BANDS],
            n_out,
            max_bin,
            // Sized to the block we expect; Vec will grow if a larger
            // block ever arrives (rare, and only re-allocates once).
            mono_buf: Vec::with_capacity(N_FFT * 2),
            fft_buf: vec![Complex::new(0.0, 0.0); N_FFT],
            spec_buf: vec![0.0; n_out],
            prev_kick_energy: 0.0,
            kick_flux: 0.0,
        }
    }

    /// Most-recent low-band spectral flux (positive half-wave). Cheap
    /// getter — the value was computed during the last `process()`
    /// call. Not smoothed, so transients stay sharp.
    pub fn kick_flux(&self) -> f32 {
        self.kick_flux
    }

    /// Process interleaved PCM samples, return (level 0-1, bands [0-1; 32]).
    /// After the call, `kick_flux()` reflects the flux computed for this
    /// block.
    pub fn process(&mut self, data: &[f32], channels: usize) -> (f32, [f32; N_BANDS]) {
        // Mono mix into the pre-allocated buffer.
        self.mono_buf.clear();
        if channels <= 1 {
            self.mono_buf.extend_from_slice(data);
        } else {
            let inv = 1.0 / channels as f32;
            for ch in data.chunks(channels) {
                self.mono_buf.push(ch.iter().sum::<f32>() * inv);
            }
        }

        // RMS
        let rms = (self.mono_buf.iter().map(|s| s * s).sum::<f32>()
            / self.mono_buf.len().max(1) as f32)
            .sqrt();

        // Fill the FFT buffer. Zero-pad or truncate to exactly N_FFT.
        for (i, slot) in self.fft_buf.iter_mut().enumerate() {
            let v = self.mono_buf.get(i).copied().unwrap_or(0.0);
            slot.re = v;
            slot.im = 0.0;
        }
        self.fft.process(&mut self.fft_buf);

        // Magnitude spectrum: |X| * 2/N (matches numpy rfft * 2/N)
        let scale = 2.0 / N_FFT as f32;
        for (i, c) in self.fft_buf[..self.n_out].iter().enumerate() {
            self.spec_buf[i] = c.norm() * scale;
        }

        // Map bins → 32 log-spaced bands (same (i/N)^0.65 mapping as Python)
        let mut raw_bands = [0.0f32; N_BANDS];
        for i in 0..N_BANDS {
            let lo = ((self.max_bin as f32 * (i as f32 / N_BANDS as f32).powf(0.65)) as usize)
                .max(1);
            let hi = ((self.max_bin as f32 * ((i + 1) as f32 / N_BANDS as f32).powf(0.65))
                as usize)
                .max(lo + 1)
                .min(self.spec_buf.len());
            raw_bands[i] = self.spec_buf[lo..hi]
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
        }

        // --- Kick-band flux (un-smoothed, un-normalized) ---
        //
        // Sum the raw low-band energies before any per-band peak
        // normalization or smoothing — we want the absolute transient
        // magnitude, not a ratio to the running peak, because the
        // ratio-form signal hides the fact that the kick is *louder*
        // than steady bass content. The beat tracker keeps its own
        // rolling average on top of this raw signal for thresholding.
        //
        // `.max(KICK_LOW_BANDS)` here is a bug-guard: N_BANDS is 32 so
        // this never clips in practice, but it makes the access safe
        // if KICK_LOW_BANDS is ever bumped above N_BANDS.
        let kick_end = KICK_LOW_BANDS.min(N_BANDS);
        let kick_energy: f32 = raw_bands[..kick_end].iter().sum();
        // Positive half-wave derivative — the textbook onset strength.
        // Negative flux (energy dropping) is useless for beat detection.
        self.kick_flux = (kick_energy - self.prev_kick_energy).max(0.0);
        self.prev_kick_energy = kick_energy;

        // RMS peak tracking + normalise (visual level path)
        if rms >= self.peak {
            self.peak = rms;
        } else {
            self.peak = (self.peak * PEAK_DECAY).max(PEAK_FLOOR);
        }
        let level = (rms / self.peak).min(1.0);
        self.smoothed_level = self.smoothed_level * 0.75 + level * 0.25;

        // Per-band peak normalise + smooth (matches Python exactly).
        // This is the smoothed visual-bands path; the beat path reads
        // the raw kick energy above, before any of this runs.
        let mut out_bands = [0.0f32; N_BANDS];
        for i in 0..N_BANDS {
            if raw_bands[i] >= self.band_peaks[i] {
                self.band_peaks[i] = raw_bands[i];
            } else {
                self.band_peaks[i] = (self.band_peaks[i] * PEAK_DECAY).max(PEAK_FLOOR);
            }
            let norm = (raw_bands[i] / self.band_peaks[i]).min(1.0);
            self.smoothed_bands[i] = self.smoothed_bands[i] * 0.55 + norm * 0.45;
            out_bands[i] = self.smoothed_bands[i];
        }

        (self.smoothed_level, out_bands)
    }
}
