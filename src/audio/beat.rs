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
/// variance (stddev < `LOCK_STDDEV_MAX`), we snap `self.interval` to the
/// nearest integer-BPM grid in `[bpm_min, bpm_max]` and start driving the
/// visible beat from a predicted grid (`last_beat + k*interval`) rather
/// than from raw flux peaks. Flux peaks in locked mode only do phase
/// correction (EMA nudge of `last_beat`) and liveness checking (drop lock
/// if the flux stream disappears or if no phase-matching peak arrives for
/// long enough — see `LOCK_DROPOUT_TIMEOUT` / `LOCK_PHASE_TIMEOUT`).
///
/// ## Iteration history (2026-04-08 → 2026-04-09)
///
/// This module has been through several rounds of empirical tuning
/// against real audio captures on pingo and on the Arch dev box. The
/// notes here record what was tried and why, so future edits don't
/// re-walk the same dead ends:
///
/// * **T6a** (`8ab0ed8`): instrumentation only. Added the 1 Hz
///   `beat-dbg:` line so we could see `ri_len`, `ri_stddev`, `bpm`,
///   `locked`, `phase_err` etc. during live playback.
/// * **T6a'** (`3abed64`): loosened `LOCK_STDDEV_MAX` from 10 ms to
///   30 ms. Pingo's first capture showed real-music `ri_stddev`
///   32–382 ms — the original 10 ms threshold was unreachable by
///   3–40×, so the tracker never locked even in the stablest passages.
/// * **T6a''** (`8e1a854`): tempo-halving heuristic applied to the
///   candidate `mean` before the lock-window range check. Idea was
///   to catch the case where the flux detector locks on
///   subdivisions (200–240 BPM sideband of a 128 BPM kick). Did not
///   solve the chaos because the halving was applied to the mean
///   only — individual samples in `recent_intervals` were still a
///   mix of real kicks, halves and doubles, pushing `ri_stddev` well
///   over 30 ms.
/// * **T6a'''** (`d9dded8`): *per-sample* octave fold into a canonical
///   octave `[min_iv, 2*min_iv)` before pushing into
///   `recent_intervals`. This finally made the lock *enter*
///   reliably — `ri_stddev` dropped into the 15–28 ms range and the
///   log started showing `beat: locked at X BPM` lines.
/// * **T6a''''** (uncommitted, superseded): once locked, drive the
///   visible beat from a predicted grid (`last_beat + k*interval`)
///   instead of firing on raw flux peaks. Flux peaks in locked mode
///   only nudge phase via an EMA and mark a rolling hit-rate window.
///   Added a `beat_hits: VecDeque<bool>` liveness check that dropped
///   the lock if fewer than 2 of the last 6 predicted beats got a
///   phase-matching flux peak. Fixed the "lock dies in 1 s from
///   3 off-grid flux peaks" regression from T6a''', but the
///   `MIN_HITS=2` check proved too strict for the DJControl monitor
///   source: flux detection was only catching ~50% of real kicks
///   plus some subdivisions, giving hit rates of 1/6 to 2/6 and
///   still dropping the lock every 2–4 seconds. Unlocked stretches
///   dominated the log, and the unlocked flux-driven path fires the
///   visual on every accepted flux peak (kicks AND subdivisions),
///   which the user perceived as "blinks on all kicks but sometimes
///   triggered in between them".
/// * **T6a⁶** (current): tuning fix on top of T6a'''''. The last
///   debug log showed locks with excellent `phase_err` (4–13 ms at
///   130–141 BPM) being killed by the 2.0 s phase timeout when a
///   neighbouring transient (~100–110 ms off grid) was getting
///   rejected and no in-tolerance flux peak happened to arrive for
///   ~2 s. Widened `PHASE_CORRECTION_TOL` 0.22 → 0.27 to absorb those
///   near-neighbour peaks as phase hits, and stretched
///   `LOCK_PHASE_TIMEOUT` 2.0 → 5.0 s so a genuine multi-beat gap in
///   phase-matching flux doesn't tear down a clean lock. A real
///   subdivision mislock still gets caught within ~10 beats.
/// * **T6a'''''**: replaces the hit-rate liveness window
///   with two timeout-based drop conditions:
///     - `LOCK_DROPOUT_TIMEOUT` (2.5 s): no accepted flux peak at
///       all → music stopped, drop.
///     - `LOCK_PHASE_TIMEOUT` (2.0 s): flux still arriving but no
///       phase-matching peak in this long → we're probably
///       phase-locked on a subdivision, drop so the next lock attempt
///       can land the grid on real kicks.
///   Also widened `PHASE_CORRECTION_TOL` from 0.15 → 0.22 to absorb
///   larger flux timing jitter on live hardware while still
///   rejecting subdivisions (which land at 0.5 fraction away from
///   any grid point). Removes the `beat_hits` VecDeque. Expected
///   effect: lock survives partial flux detection, so the unlocked
///   path stops dominating — the visual pulses stay aligned to real
///   kicks for long continuous stretches.
use std::collections::VecDeque;

const HISTORY: usize = 43;
const COOLDOWN: f64 = 0.22;

/// How many consecutive well-spaced beats we need before locking.
const LOCK_WINDOW: usize = 8;
/// Maximum stddev of inter-beat intervals (seconds) inside the lock
/// window. Previously 0.010 (10 ms), which was derived from theory
/// without real-audio data. The T6a beat-dbg dump collected on pingo
/// on 2026-04-08 showed `ri_stddev` ranging 32–382 ms (mean 111 ms) on
/// real mixed music — the 10 ms threshold was unreachable by a factor
/// of 3–40×, explaining 0 lock events across ~47 minutes of capture.
/// 30 ms is the new ceiling: still tight enough that a steady 4/4 kick
/// fits comfortably (the best observed moments touched ~32 ms, so a
/// clean studio loop should dip below), but achievable on live mixed
/// material. Expect pingo's next vjtest to actually log `beat: locked
/// at X BPM` lines.
const LOCK_STDDEV_MAX: f64 = 0.030;

/// T6a''''' (2026-04-09): PLL phase-locked maintenance constants.
///
/// Once locked, the visible beat fires on the prediction grid and flux
/// peaks are used only for phase correction + liveness checking. See
/// the locked branch of `update()` for the full design, and the module
/// doc comment at the top of the file for the iteration history.

/// Phase tolerance (fraction of `interval`) for accepting a flux peak as
/// a "hit" on a predicted beat. 27% at 130 BPM ≈ ±125 ms — wide enough
/// to absorb the timing jitter of a real-hardware flux detector on
/// live mixed audio, still comfortably narrower than a subdivision
/// (0.5 fraction away from any grid point). T6a⁶ widened from 0.22
/// after observing phase_err staying at 4–13 ms on clean locks while
/// a neighbouring flux peak (another transient nearby) was being
/// rejected at ~100–110 ms and running out the phase timeout.
const PHASE_CORRECTION_TOL: f64 = 0.27;

/// EMA α for nudging `last_beat` toward the measured flux-peak phase.
/// 0.10 tracks drift of ~1 BPM/s comfortably without reacting to
/// single-beat timing noise.
const PHASE_CORRECTION_ALPHA: f64 = 0.10;

/// Drop the lock if no flux peak has been accepted at all in this many
/// seconds. Indicates the music has stopped or the source has gone
/// silent. 2.5 s is roughly 5–6 beats at EDM tempi — long enough to
/// ride through a riser/breakdown, short enough that a true dropout
/// is caught quickly.
const LOCK_DROPOUT_TIMEOUT: f64 = 2.5;

/// Drop the lock if flux peaks are still arriving but none of them
/// have landed within `PHASE_CORRECTION_TOL` of a predicted beat for
/// this many seconds. Indicates the grid is phase-mislocked (almost
/// always: we locked on a subdivision instead of the downbeat, so the
/// grid fires halfway between real kicks). T6a⁶ widened from 2.0 → 5.0
/// after a log showed clean locks with `phase_err` stable at 4–13 ms
/// being killed because a 1–2 s gap in phase-matching flux peaks ran
/// out the old 2.0 s timeout. 5.0 s ≈ 10 beats at 130 BPM — still short
/// enough to catch a real subdivision mislock within one or two bars.
const LOCK_PHASE_TIMEOUT: f64 = 5.0;

#[derive(Clone, Debug)]
pub struct BeatState {
    pub beat: bool,
    pub half_beat: bool,
    pub quarter_beat: bool,
    pub bpm: f32,
    /// True when the tempo tracker has engaged a stable lock (see the
    /// `locked` branch of `update()`). Consumers can use this to
    /// distinguish the live grid-driven BPM from the pre-lock seed
    /// estimate — e.g. the HUD draws a ★ indicator next to the BPM
    /// readout when locked, and `~` when still tracking.
    pub locked: bool,
}

pub struct BeatTracker {
    sensitivity: f32,
    /// Rolling history of recent flux samples for adaptive thresholding.
    flux_history: VecDeque<f32>,
    /// Timestamp of the most recently fired main beat. In unlocked mode
    /// this is the time of the last flux peak that passed the threshold;
    /// in locked mode it is a point on the prediction grid, advanced by
    /// one `interval` each time `update()` crosses the next predicted
    /// beat, and gently nudged via EMA by flux peaks that land near a
    /// predicted beat (T6a'''' PLL).
    last_beat: f64,
    /// Current best estimate of inter-beat interval (seconds). When
    /// `locked` is true this is frozen at the snapped BPM grid point.
    interval: f64,
    locked: bool,
    consec_in_window: usize,
    recent_intervals: VecDeque<f64>,
    /// Bitfield tracking which subdivisions of the current beat already
    /// fired. Bit 0 = main beat, bit 1 = half beat (1/8), bit 2 = first
    /// quarter (1/16 at 1/4 position), bit 3 = third quarter (1/16 at
    /// 3/4 position). Replaces a `HashSet<u8>` that allocated on every
    /// beat.
    sub_fired: u8,
    /// BPM lock window (inclusive, in beats per minute).
    bpm_min: f32,
    bpm_max: f32,
    /// T6a'''' PLL state: timestamp of the most recent flux peak we
    /// actually accepted (passed the sensitivity + cooldown gate). Used
    /// as the cooldown anchor; in locked mode `last_beat` advances on
    /// the prediction grid so it can't be reused for cooldown. Also
    /// drives the `LOCK_DROPOUT_TIMEOUT` dropout check.
    last_flux_peak: f64,
    /// T6a''''' PLL state: timestamp of the most recent flux peak that
    /// landed within `PHASE_CORRECTION_TOL` of a predicted beat in
    /// locked mode. Drives the `LOCK_PHASE_TIMEOUT` phase-mislock
    /// detection — if flux is still arriving but none of it is hitting
    /// the grid, we're locked on the wrong phase and need to drop.
    last_phase_hit: f64,
    /// T6a'''' PLL state: running EMA of signed phase error between
    /// flux peaks and predicted beats (seconds). Diagnostic only; logged
    /// in the 1 Hz `beat-dbg` line.
    phase_err_ema: f64,
    /// Frame counter for T6a instrumentation — throttles the periodic
    /// debug dump in `update()` to once per ~1 s at 60 Hz audio updates.
    /// Measurement only; does not affect tracker behavior.
    dbg_frame: u32,
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
            sub_fired: 0,
            bpm_min: 120.0,
            bpm_max: 160.0,
            last_flux_peak: -10.0,
            last_phase_hit: -10.0,
            phase_err_ema: 0.0,
            dbg_frame: 0,
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
        self.phase_err_ema = 0.0;
        // Do not reset last_phase_hit / last_flux_peak; they're used
        // by the new lock attempt to bootstrap its timeouts naturally
        // after re-locking.
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

    /// T6a''' (2026-04-09): per-sample octave fold into the BPM window.
    ///
    /// The raw interval series from a flux detector on live music is a
    /// chaotic mix of true kick→kick intervals, snare/hat false positives
    /// that halve the interval, and missed kicks that double it. Pushing
    /// all three into `recent_intervals` unfolded made `ri_stddev` 180–450
    /// ms on the DJControl monitor capture (2026-04-09 arch session),
    /// 6–15× the 30 ms lock threshold — the tracker could never lock.
    ///
    /// Folding each sample into the canonical octave `[min_iv, 2*min_iv)`
    /// before pushing collapses the 3 populations into one cluster around
    /// the true tempo. Samples that fold into `[min_iv, max_iv]` are
    /// accepted; samples whose canonical residue lands above `max_iv`
    /// (tempos outside the lock window, e.g. 108 BPM under a [120,160]
    /// window) are rejected so they can't pollute the stddev.
    ///
    /// Bounded to 8 iterations to guarantee termination on any finite
    /// input. For an interval already inside the canonical octave the
    /// loop is a no-op.
    fn fold_to_window(&self, iv: f64) -> Option<f64> {
        let min_iv = 60.0 / self.bpm_max as f64;
        let max_iv = 60.0 / self.bpm_min as f64;
        let canonical_hi = 2.0 * min_iv;
        let mut x = iv;
        for _ in 0..8 {
            if x >= canonical_hi {
                x *= 0.5;
            } else if x < min_iv {
                x *= 2.0;
            } else {
                break;
            }
        }
        if x >= min_iv && x <= max_iv {
            Some(x)
        } else {
            None
        }
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

        // --- Locked mode: grid-driven beat prediction ---
        //
        // T6a'''' (2026-04-09): once the tracker is locked, the visible
        // beat fires on the prediction grid (`last_beat + k*interval`),
        // NOT on raw flux peaks. This fixes two problems that persisted
        // through T6a''' on the Arch/DJControl monitor capture:
        //   1. `beat = true` was set on every accepted flux peak, so
        //      subdivision false positives (hats, snares) produced
        //      visible double beats between real kicks.
        //   2. The locked drift check ate raw elapsed intervals, so 3
        //      off-grid flux peaks in a row killed the lock within ~1 s.
        // Both go away by decoupling the visual pulse from the flux
        // stream once we trust the tempo.
        //
        // In practice this `while` runs 0 or 1 iterations per frame
        // (frame ~16 ms ≪ beat ~450 ms). The loop form is defensive
        // against a starved frame.
        if self.locked {
            while t >= self.last_beat + self.interval {
                self.last_beat += self.interval;
                beat = true;
                self.sub_fired = 0b0001;
            }
        }

        // --- Flux-peak handling ---
        //
        // Threshold on flux, not on smoothed RMS. `flux > avg * sensitivity`
        // picks the transient peaks — the kick — while ignoring steady
        // bass content that would raise the RMS without providing a
        // musical beat. Cooldown anchored on `last_flux_peak` (not
        // `last_beat`) because in locked mode `last_beat` advances on
        // the grid and would keep the cooldown gate permanently open.
        if flux > avg * self.sensitivity
            && (t - self.last_flux_peak) > COOLDOWN
        {
            self.last_flux_peak = t;

            if self.locked {
                // Locked path: use the flux peak for phase correction +
                // liveness marking only. Do NOT set `beat = true` here —
                // the visible beat is already driven from the grid
                // prediction block above.
                //
                // Find the closest predicted beat: either the one we
                // just fired (`last_beat`) or the next one
                // (`last_beat + interval`). The signed phase error `err`
                // is the distance from that prediction to `t`. If the
                // flux peak is within ±PHASE_CORRECTION_TOL of the
                // closest predicted beat, count it as a hit and nudge
                // `last_beat` toward the measured phase by α. Otherwise
                // the flux peak is a subdivision/false positive — ignore
                // it entirely.
                let phase_to_current = t - self.last_beat;
                let phase_to_next = (self.last_beat + self.interval) - t;
                let (err, target) = if phase_to_current.abs() <= phase_to_next.abs() {
                    (phase_to_current, t)
                } else {
                    (-phase_to_next, t - self.interval)
                };

                if err.abs() < self.interval * PHASE_CORRECTION_TOL {
                    self.last_beat = self.last_beat * (1.0 - PHASE_CORRECTION_ALPHA)
                        + target * PHASE_CORRECTION_ALPHA;
                    self.last_phase_hit = t;
                    self.phase_err_ema = self.phase_err_ema * 0.9 + err * 0.1;
                }
            } else {
                // Unlocked path: flux peak drives the visible beat and
                // feeds the lock-window evaluator. Unchanged from T6a'''.
                let elapsed = t - self.last_beat;
                beat = true;

                if elapsed > 0.2 && elapsed < 2.5 {
                    // Free-floating EMA fallback.
                    self.interval = self.interval * 0.85 + elapsed * 0.15;

                    // Feed the lock-window evaluator. T6a''' (2026-04-09):
                    // per-sample octave fold into the BPM window before
                    // pushing. This turns a chaotic mix of true beats,
                    // snare/hat halves and missed-kick doubles into a
                    // tight tempo cluster. Samples whose canonical
                    // octave residue falls outside [min_iv, max_iv] are
                    // rejected so they can't raise the window stddev.
                    if let Some(folded) = self.fold_to_window(elapsed) {
                        if self.recent_intervals.len() >= LOCK_WINDOW {
                            self.recent_intervals.pop_front();
                        }
                        self.recent_intervals.push_back(folded);

                        if self.recent_intervals.len() >= LOCK_WINDOW {
                            // Compute mean + stddev over the pre-folded
                            // window. If stable, snap and lock.
                            let n = self.recent_intervals.len() as f64;
                            let mean = self.recent_intervals.iter().sum::<f64>() / n;
                            let var = self
                                .recent_intervals
                                .iter()
                                .map(|x| (x - mean) * (x - mean))
                                .sum::<f64>()
                                / n;
                            let stddev = var.sqrt();

                            if stddev < LOCK_STDDEV_MAX {
                                let snapped = self.snap_interval_to_grid(mean);
                                self.interval = snapped;
                                self.locked = true;
                                self.phase_err_ema = 0.0;
                                // Bootstrap the phase-timeout clock: the
                                // lock-triggering flux peak counts as
                                // the initial phase hit.
                                self.last_phase_hit = t;
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
                // Reset subdivision bitfield for the new beat; bit 0 =
                // main beat fired.
                self.sub_fired = 0b0001;
            }
        }

        // --- Liveness check (locked mode only) ---
        //
        // T6a''''' replaces the earlier rolling-hit-rate liveness window
        // with two independent timeout-based drop conditions:
        //
        //   1. LOCK_DROPOUT_TIMEOUT — no accepted flux peak at all for
        //      this long. The music has stopped or the audio source has
        //      gone silent. Drop and let the unlocked path re-measure
        //      when flux returns.
        //
        //   2. LOCK_PHASE_TIMEOUT — flux peaks are still arriving but
        //      none of them have landed in the phase window. Almost
        //      always this means we locked on a subdivision rather than
        //      the downbeat, and the grid is firing halfway between
        //      real kicks. Drop so the next lock attempt can land the
        //      grid on a different flux peak and (statistically) catch
        //      the correct phase.
        //
        // The hit-rate window failed on the DJControl monitor source
        // because flux detection only caught ~50 % of real kicks; hit
        // rate bounced around 1–2/6 and the lock died every 2–4 s.
        // Timeout-based checks are decoupled from the detection rate
        // and stay green as long as *some* flux is arriving.
        if self.locked {
            if t - self.last_flux_peak > LOCK_DROPOUT_TIMEOUT {
                log::info!(
                    "beat: lock dropped (no flux for {:.1}s)",
                    t - self.last_flux_peak
                );
                self.drop_lock();
            } else if t - self.last_phase_hit > LOCK_PHASE_TIMEOUT {
                log::info!(
                    "beat: lock dropped (no phase hit for {:.1}s — probable subdivision mislock)",
                    t - self.last_phase_hit
                );
                self.drop_lock();
            }
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

        // T6a instrumentation — periodic dump of the tracker's internal
        // state so a known-BPM track run on pingo can tell us why the
        // lock never engages in real sets. Measurement only; tune
        // LOCK_STDDEV_MAX / LOCK_WINDOW in a later commit after reading
        // the log. Throttled to ~1 Hz at 60 Hz update rate.
        self.dbg_frame = self.dbg_frame.wrapping_add(1);
        if self.dbg_frame % 60 == 0 {
            let flux_peak = self
                .flux_history
                .iter()
                .copied()
                .fold(0.0f32, f32::max);
            let (ri_mean, ri_stddev) = if self.recent_intervals.len() >= 2 {
                let n = self.recent_intervals.len() as f64;
                let m = self.recent_intervals.iter().sum::<f64>() / n;
                let v = self
                    .recent_intervals
                    .iter()
                    .map(|x| (x - m) * (x - m))
                    .sum::<f64>()
                    / n;
                (m, v.sqrt())
            } else {
                (0.0, 0.0)
            };
            let since_flux = t - self.last_flux_peak;
            let since_phase = t - self.last_phase_hit;
            log::debug!(
                "beat-dbg: flux_peak={:.4} interval={:.4}s bpm={:.1} locked={} ri_len={} ri_mean={:.4}s ri_stddev={:.1}ms phase_err={:.1}ms since_flux={:.2}s since_phase={:.2}s",
                flux_peak,
                self.interval,
                bpm,
                self.locked,
                self.recent_intervals.len(),
                ri_mean,
                ri_stddev * 1000.0,
                self.phase_err_ema * 1000.0,
                since_flux,
                since_phase
            );
        }

        BeatState { beat, half_beat, quarter_beat, bpm, locked: self.locked }
    }
}
