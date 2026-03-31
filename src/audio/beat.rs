/// Exact port of Python BeatTracker.
/// Transient detector with half/quarter-beat subdivision.
use std::collections::{HashSet, VecDeque};

const HISTORY: usize = 43;
const COOLDOWN: f64 = 0.22;

#[derive(Clone, Debug)]
pub struct BeatState {
    pub beat: bool,
    pub half_beat: bool,
    pub quarter_beat: bool,
    pub bpm: f32,
}

pub struct BeatTracker {
    sensitivity: f32,
    history: VecDeque<f32>,
    last_beat: f64,
    interval: f64,
    sub_fired: HashSet<u8>,
}

impl BeatTracker {
    pub fn new(sensitivity: f32) -> Self {
        Self {
            sensitivity,
            history: VecDeque::with_capacity(HISTORY),
            last_beat: -10.0,
            interval: 0.5,
            sub_fired: HashSet::new(),
        }
    }

    pub fn set_sensitivity(&mut self, s: f32) {
        self.sensitivity = s.clamp(0.5, 3.0);
    }

    pub fn update(&mut self, level: f32, t: f64) -> BeatState {
        if self.history.len() >= HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(level);

        let avg = (self.history.iter().sum::<f32>() / self.history.len() as f32).max(1e-6);
        let mut beat = false;

        if level > avg * self.sensitivity && (t - self.last_beat) > COOLDOWN {
            beat = true;
            let elapsed = t - self.last_beat;
            if elapsed > 0.2 && elapsed < 2.5 {
                self.interval = self.interval * 0.85 + elapsed * 0.15;
            }
            self.last_beat = t;
            self.sub_fired = [0].into();
        }

        let mut half_beat = beat;
        let mut quarter_beat = beat;

        if self.last_beat > 0.0 {
            let e = t - self.last_beat;
            let iv = self.interval;

            if e >= iv * 0.45 && !self.sub_fired.contains(&1) {
                self.sub_fired.insert(1);
                half_beat = true;
                quarter_beat = true;
            }
            for (idx, frac) in [(2u8, 0.22f64), (3, 0.72)] {
                if e >= iv * frac && !self.sub_fired.contains(&idx) {
                    self.sub_fired.insert(idx);
                    quarter_beat = true;
                }
            }
        }

        let bpm = 60.0 / self.interval as f32;

        BeatState { beat, half_beat, quarter_beat, bpm }
    }
}
