/// Exact port of Python AudioAnalyzer._cb() logic.
/// Processes audio blocks → RMS level + 32 log-spaced frequency bands.
use rustfft::{num_complex::Complex, FftPlanner};

use super::state::N_BANDS;

const BLOCK_SIZE: usize = 512;
const N_FFT: usize = 256;
const PEAK_FLOOR: f32 = 0.001;
const PEAK_DECAY: f32 = 0.9997;

pub struct AudioAnalyzer {
    fft_planner: FftPlanner<f32>,
    peak: f32,
    band_peaks: [f32; N_BANDS],
    smoothed_level: f32,
    smoothed_bands: [f32; N_BANDS],
    sample_rate: u32,
    n_out: usize,
    max_bin: usize,
}

impl AudioAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        let n_out = N_FFT / 2 + 1; // rfft output bins
        let max_bin = (n_out as f32 * 14000.0 / 22050.0) as usize;
        let max_bin = max_bin.max(2).min(n_out);
        Self {
            fft_planner: FftPlanner::new(),
            peak: PEAK_FLOOR,
            band_peaks: [PEAK_FLOOR; N_BANDS],
            smoothed_level: 0.0,
            smoothed_bands: [0.0; N_BANDS],
            sample_rate,
            n_out,
            max_bin,
        }
    }

    /// Process interleaved PCM samples, return (level 0-1, bands [0-1; 32]).
    pub fn process(&mut self, data: &[f32], channels: usize) -> (f32, [f32; N_BANDS]) {
        // Mono mix
        let mono: Vec<f32> = if channels <= 1 {
            data.iter().cloned().collect()
        } else {
            data.chunks(channels)
                .map(|ch| ch.iter().sum::<f32>() / channels as f32)
                .collect()
        };

        // RMS
        let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();

        // FFT buffer (256 samples, zero-padded)
        let mut buf: Vec<Complex<f32>> = (0..N_FFT)
            .map(|i| Complex::new(*mono.get(i).unwrap_or(&0.0), 0.0))
            .collect();

        let fft = self.fft_planner.plan_fft_forward(N_FFT);
        fft.process(&mut buf);

        // Magnitude spectrum: |X| * 2/N (matches numpy rfft * 2/N)
        let spec: Vec<f32> = buf[..self.n_out]
            .iter()
            .map(|c| c.norm() * 2.0 / N_FFT as f32)
            .collect();

        // Map bins → 32 log-spaced bands (same (i/N)^0.65 mapping as Python)
        let mut raw_bands = [0.0f32; N_BANDS];
        for i in 0..N_BANDS {
            let lo = ((self.max_bin as f32 * (i as f32 / N_BANDS as f32).powf(0.65)) as usize)
                .max(1);
            let hi = ((self.max_bin as f32 * ((i + 1) as f32 / N_BANDS as f32).powf(0.65))
                as usize)
                .max(lo + 1)
                .min(spec.len());
            raw_bands[i] = spec[lo..hi].iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        }

        // RMS peak tracking + normalise
        if rms >= self.peak {
            self.peak = rms;
        } else {
            self.peak = (self.peak * PEAK_DECAY).max(PEAK_FLOOR);
        }
        let level = (rms / self.peak).min(1.0);
        self.smoothed_level = self.smoothed_level * 0.75 + level * 0.25;

        // Per-band peak normalise + smooth (matches Python exactly)
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
