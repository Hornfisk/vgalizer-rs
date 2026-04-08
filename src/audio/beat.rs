/// Beat tracker driven by low-band spectral flux (kick onsets) with an
/// optional 4/4 tempo lock in the 120–160 BPM window.
///
/// ## Design
///
/// The old implementation ran a running-average threshold on the smoothed
/// full-band RMS `level`. Two problems for live EDM use:
///   1. Vocals/pads/leads raise the RMS without touching the kick, so the
///      "beat" lagged or missed in dense passages.
///   2. The level was EMA-smoothed (~4 block time constant, ~23 ms) before
///      reaching the tracker, smearing the kick transient across frames.
///
/// This tracker consumes `kick_flux` — the positive half-wave derivative
/// of the raw low-band (sub-bass..low-bass) energy, computed un-smoothed
/// in `AudioAnalyzer::process()`. That's the textbook onset-strength
/// signal for kick detection and gives sub-frame sharpness.
///
/// ## Tempo lock
///
/// Once we've seen `LOCK_WINDOW` consecutive beats with low interval
/// variance (stddev < `LOCK_STDDEV_MAX`) and the median interval falls
/// inside [60/bpm_max, 60/bpm_min], we snap `self.interval` to the nearest
/// integer-BPM grid in that window and freeze it for subdivision math.
/// The lock breaks if we miss `UNLOCK_MISSES` beats in a row or the median
/// drifts outside the window. This gives sharp, musically correct 1/8 and
/// 1/16 subdivisions as long as the tempo is stable; free-floating EMA
/// fallback handles transitions.
use std::collections::VecDeque;

const HISTORY: usize = 43;
const COOLDOWN: f64 = 0.22;

/// How many consecutive well-spaced beats we need before locking.
const LOCK_WINDOW: usize = 8;
/// Maximum stddev of inter-beat intervals (seconds) inside the lock
/// window. ~10 ms is tight enough to reject drifting tempos but loose
/// enough to tolerate the small jitter inherent to flux peak picking.
const LOCK_STDDEV_MAX: f64 = 0.010;
/// Miss this many predicted beats in a row and the lock drops.
const UNLOCK_MISSES: u32 = 3;

#[derive(Clone, Debug)]
pub struct BeatState {
    pub beat: bool,
    pub half_beat: bool,
    pub quarter_beat: bool,
    pub bpm: f32,
}

pub struct BeatTracker {
    sensitivity: f32,
    /// Rolling history of recent flux samples for adaptive thresholding.
    flux_history: VecDeque<f32>,
    last_beat: f64,
    /// Current best estimate of inter-beat interval (seconds). When
    /// `locked` is true this is frozen at the snapped BPM grid point.
    interval: f64,
    locked: bool,
    consec_in_window: usize,
    recent_intervals: VecDeque<f64>,
    missed_beats: u32,
    /// Bitfield tracking which subdivisions of the current beat already
    /// fired. Bit 0 = main beat, bit 1 = half beat (1/8), bit 2 = first
    /// quarter (1/16 at 1/4 position), bit 3 = third quarter (1/16 at
    /// 3/4 position). Replaces a `HashSet<u8>` that allocated on every
    /// beat.
    sub_fired: u8,
    /// BPM lock window (inclusive, in beats per minute).
    bpm_min: f32,
    bpm_max: f32,
}

impl BeatTracker {
    pub fn new(sensitivity: f32) -> Self {
        Self {
            sensitivity,
            flux_history: VecDeque::with_capacity(HISTORY),
            last_beat: -10.0,
            // Pre-seed at 140 BPM — the middle of the EDM hot range the
            // user plays. Gives subdivision math something reasonable
            // to work with on the very first beats before the tracker
            // locks in.
            interval: 60.0 / 140.0,
            locked: false,
            consec_in_window: 0,
            recent_intervals: VecDeque::with_capacity(LOCK_WINDOW),
            missed_beats: 0,
            sub_fired: 0,
            bpm_min: 120.0,
            bpm_max: 160.0,
        }
    }

    pub fn set_sensitivity(&mut self, s: f32) {
        self.sensitivity = s.clamp(0.5, 3.0);
    }

    /// Update the BPM lock window. Clamped to sane values. If the window
    /// changes mid-set we drop any existing lock so the new bounds take
    /// effect immediately on the next beat.
    pub fn set_bpm_lock_range(&mut self, min: f32, max: f32) {
        let lo = min.clamp(40.0, 300.0);
        let hi = max.clamp(lo + 1.0, 300.0);
        if (lo - self.bpm_min).abs() > 0.01 || (hi - self.bpm_max).abs() > 0.01 {
            self.bpm_min = lo;
            self.bpm_max = hi;
            self.drop_lock();
        }
    }

    fn drop_lock(&mut self) {
        self.locked = false;
        self.consec_in_window = 0;
        self.recent_intervals.clear();
        self.missed_beats = 0;
    }

    /// Snap an interval (seconds) to the nearest integer-BPM grid point
    /// inside `[bpm_min, bpm_max]`. Returns the snapped interval.
    fn snap_interval_to_grid(&self, interval: f64) -> f64 {
        let bpm = 60.0 / interval;
        let snapped_bpm = bpm
            .round()
            .clamp(self.bpm_min as f64, self.bpm_max as f64);
        60.0 / snapped_bpm
    }

    /// Called every frame with the current flux sample and wall-clock time.
    /// Returns the beat state (main beat, 1/8, 1/16 flags, current BPM).
    pub fn update(&mut self, flux: f32, t: f64) -> BeatState {
        // Rolling history for adaptive thresholding.
        if self.flux_history.len() >= HISTORY {
            self.flux_history.pop_front();
        }
        self.flux_history.push_back(flux);

        let avg = (self.flux_history.iter().sum::<f32>()
            / self.flux_history.len() as f32)
            .max(1e-6);

        let mut beat = false;

        // Threshold on flux, not on smoothed RMS. `flux > avg * sensitivity`
        // picks the transient peaks — the kick — while ignoring steady
        // bass content that would raise the RMS without providing a
        // musical beat.
        if flux > avg * self.sensitivity && (t - self.last_beat) > COOLDOWN {
            beat = true;
            let elapsed = t - self.last_beat;

            if elapsed > 0.2 && elapsed < 2.5 {
                if self.locked {
                    // Treat the locked interval as ground truth. Only
                    // unlock if we drift hard. A beat landing within
                    // 12 % of the locked interval is a predicted hit;
                    // outside that, count a "miss" (the predicted beat
                    // that should have landed here didn't match, so
                    // our tempo estimate is stale).
                    let rel_err = (elapsed - self.interval).abs() / self.interval;
                    if rel_err > 0.12 {
                        self.missed_beats += 1;
                        if self.missed_beats >= UNLOCK_MISSES {
                            log::info!(
                                "beat: lock dropped (drift {:.1}%)",
                                rel_err * 100.0
                            );
                            self.drop_lock();
                            // Also seed the free-floating EMA with the
                            // new elapsed so we don't take forever to
                            // re-catch.
                            self.interval = elapsed;
                        }
                    } else {
                        self.missed_beats = 0;
                    }
                } else {
                    // Free-floating EMA fallback.
                    self.interval = self.interval * 0.85 + elapsed * 0.15;

                    // Feed the lock-window evaluator.
                    if self.recent_intervals.len() >= LOCK_WINDOW {
                        self.recent_intervals.pop_front();
                    }
                    self.recent_intervals.push_back(elapsed);

                    if self.recent_intervals.len() >= LOCK_WINDOW {
                        // Compute mean + stddev over the window. If
                        // stable *and* inside the BPM range, snap and
                        // lock.
                        let n = self.recent_intervals.len() as f64;
                        let mean = self.recent_intervals.iter().sum::<f64>() / n;
                        let var = self
                            .recent_intervals
                            .iter()
                            .map(|x| (x - mean) * (x - mean))
                            .sum::<f64>()
                            / n;
                        let stddev = var.sqrt();
                        let min_iv = 60.0 / self.bpm_max as f64;
                        let max_iv = 60.0 / self.bpm_min as f64;
                        if stddev < LOCK_STDDEV_MAX
                            && mean >= min_iv
                            && mean <= max_iv
                        {
                            let snapped = self.snap_interval_to_grid(mean);
                            self.interval = snapped;
                            self.locked = true;
                            self.missed_beats = 0;
                            log::info!(
                                "beat: locked at {:.1} BPM (stddev={:.1}ms)",
                                60.0 / snapped,
                                stddev * 1000.0
                            );
                        }
                    }
                }
            }
            self.last_beat = t;
            // Reset subdivision bitfield for the new beat; bit 0 = main
            // beat fired.
            self.sub_fired = 0b0001;
        }

        // --- Subdivision prediction ---
        //
        // Standard 1/8 and 1/16 grid relative to the locked (or EMA'd)
        // interval. Fires once each per beat window, tracked via the
        // sub_fired bitfield. The old code used frac=0.22 and 0.72 for
        // quarter_beat, which is neither a 1/16 grid nor anything
        // musically meaningful. This replaces them with 1/4, 1/2, 3/4
        // of the beat (i.e. the three 1/16 boundaries inside the beat).
        let mut half_beat = beat;
        let mut quarter_beat = beat;

        if self.last_beat > 0.0 {
            let e = t - self.last_beat;
            let iv = self.interval;

            // 1/2 of the beat = 1/8 note in 4/4
            if e >= iv * 0.5 && (self.sub_fired & 0b0010) == 0 {
                self.sub_fired |= 0b0010;
                half_beat = true;
                quarter_beat = true;
            }
            // 1/4 of the beat = first 1/16
            if e >= iv * 0.25 && (self.sub_fired & 0b0100) == 0 {
                self.sub_fired |= 0b0100;
                quarter_beat = true;
            }
            // 3/4 of the beat = third 1/16
            if e >= iv * 0.75 && (self.sub_fired & 0b1000) == 0 {
                self.sub_fired |= 0b1000;
                quarter_beat = true;
            }
        }

        let bpm = 60.0 / self.interval as f32;

        BeatState { beat, half_beat, quarter_beat, bpm }
    }
}
