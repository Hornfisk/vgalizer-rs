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
/// Default cooldown between accepted beats (seconds). Matches a max
/// detectable BPM of 60/0.22 ≈ 272. At runtime this is overridden by
/// `BeatTracker::cooldown`, which is driven live by the `bpm_lock_max`
/// knob in the vje overlay (T6a⁸: repurposed as "max detectable BPM"
/// since the PLL lock is gone).
const DEFAULT_COOLDOWN: f64 = 0.22;

/// Kept for `recent_intervals` VecDeque capacity so the struct layout
/// doesn't change even though T6a⁸ no longer uses the lock window.
const LOCK_WINDOW: usize = 8;

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
    /// T6a⁸ (2026-04-10): minimum seconds between accepted flux peaks.
    /// Live-tunable via `set_bpm_lock_range(_, max)` where `max` is
    /// reinterpreted as "max detectable BPM". Raising it filters out
    /// subdivision false-positives (hats, snares) without touching
    /// sensitivity, which only sets the flux magnitude threshold.
    cooldown: f64,
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
            recent_intervals: VecDeque::with_capacity(LOCK_WINDOW),
            sub_fired: 0,
            bpm_min: 120.0,
            bpm_max: 160.0,
            last_flux_peak: -10.0,
            last_phase_hit: -10.0,
            phase_err_ema: 0.0,
            dbg_frame: 0,
            cooldown: DEFAULT_COOLDOWN,
        }
    }

    pub fn set_sensitivity(&mut self, s: f32) {
        self.sensitivity = s.clamp(0.1, 5.0);
    }

    /// T6a⁸ (2026-04-10): `min` is ignored (the PLL lock window is
    /// gone). `max` is reinterpreted as **max detectable BPM** and
    /// directly drives the cooldown between accepted flux peaks. Raise
    /// it to let the detector blink faster (catches more hats/subs);
    /// lower it to clamp the blink rate and reject subdivision
    /// false-positives. 180 = 333 ms cooldown is a good starting point
    /// for 4/4 EDM where real kicks rarely exceed ~170 BPM.
    pub fn set_bpm_lock_range(&mut self, min: f32, max: f32) {
        let hi = max.clamp(40.0, 600.0);
        self.bpm_min = min;
        self.bpm_max = hi;
        self.cooldown = 60.0 / hi as f64;
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

        // --- Direct onset-triggered beat (T6a⁸, 2026-04-10) ---
        //
        // The PLL / tempo-lock path was removed after multiple rounds of
        // empirical tuning failed to lock reliably on real mixed music
        // from a DJ controller. The lock window repeatedly snapped to
        // subdivisions or partial-fractional tempos (9/8 inflation etc.)
        // because real-hardware flux only catches a fraction of kicks
        // plus stray subdivision transients, and the stddev of the
        // resulting interval set was too noisy for any reasonable grid
        // snap to survive more than a few seconds.
        //
        // New behaviour: every flux peak that clears the adaptive
        // threshold + cooldown fires `beat = true` directly. No grid
        // prediction, no phase correction, no lock state. The visual
        // pulse is exactly the onset detector output — if the detector
        // misses a kick the visual misses it too, and if it catches a
        // hat the visual pulses on the hat. That's the honest signal,
        // and it's dramatically more reliable than a drifting PLL.
        //
        // `interval` is still EMA-updated so the HUD BPM readout and
        // the subdivision timing (1/8, 1/16) below remain meaningful.
        if flux > avg * self.sensitivity
            && (t - self.last_beat) > self.cooldown
        {
            let elapsed = t - self.last_beat;
            if elapsed > 0.2 && elapsed < 2.5 {
                self.interval = self.interval * 0.85 + elapsed * 0.15;
            }
            self.last_beat = t;
            self.last_flux_peak = t;
            beat = true;
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

        BeatState { beat, half_beat, quarter_beat, bpm, locked: false }
    }

    /// Advance the tracker's subdivision state WITHOUT consuming a flux
    /// sample. Used by the render thread between drained audio-thread
    /// flux samples so the 1/8 and 1/16 pulses fire at render wall-clock
    /// rather than being quantized to audio-block arrival. Post-T6a⁸
    /// there is no locked-grid prediction, so this only drains the
    /// subdivision bitfield.
    pub fn tick(&mut self, t: f64) -> BeatState {
        let beat = false;
        let mut half_beat = false;
        let mut quarter_beat = false;
        if self.last_beat > 0.0 {
            let e = t - self.last_beat;
            let iv = self.interval;
            if e >= iv * 0.5 && (self.sub_fired & 0b0010) == 0 {
                self.sub_fired |= 0b0010;
                half_beat = true;
                quarter_beat = true;
            }
            if e >= iv * 0.25 && (self.sub_fired & 0b0100) == 0 {
                self.sub_fired |= 0b0100;
                quarter_beat = true;
            }
            if e >= iv * 0.75 && (self.sub_fired & 0b1000) == 0 {
                self.sub_fired |= 0b1000;
                quarter_beat = true;
            }
        }

        let bpm = 60.0 / self.interval as f32;
        BeatState { beat, half_beat, quarter_beat, bpm, locked: false }
    }
}
