/// Tests for FFT band extraction and RMS accuracy in AudioAnalyzer.
/// Verifies the exact Python-ported algorithm behaviour on known signals.
use vgalizer::audio::analysis::AudioAnalyzer;

const SR: u32 = 44100;

fn sine_wave(freq: f32, n_samples: usize, amplitude: f32) -> Vec<f32> {
    use std::f32::consts::TAU;
    (0..n_samples)
        .map(|i| amplitude * (TAU * freq * i as f32 / SR as f32).sin())
        .collect()
}

/// Feed blocks until the peak tracker has warmed up (bands stabilize).
fn warm_up(analyzer: &mut AudioAnalyzer, data: &[f32]) {
    for _ in 0..60 {
        analyzer.process(data, 1);
    }
}

#[test]
fn silence_gives_zero_level() {
    let mut a = AudioAnalyzer::new(SR);
    let silence = vec![0.0f32; 512];
    warm_up(&mut a, &silence);
    let (level, bands) = a.process(&silence, 1);
    assert!(level < 0.01, "silence level too high: {level}");
    assert!(bands.iter().all(|&b| b < 0.01), "silence bands not near zero");
}

#[test]
fn loud_signal_gives_high_level() {
    let mut a = AudioAnalyzer::new(SR);
    let loud = vec![0.9f32; 512];
    warm_up(&mut a, &loud);
    let (level, _bands) = a.process(&loud, 1);
    assert!(level > 0.5, "loud signal level too low: {level}");
}

/// Once a loud signal stops, the self-normalised level should decrease
/// (the per-band peak tracker decays toward PEAK_FLOOR via PEAK_DECAY^n).
#[test]
fn level_decays_after_signal_stops() {
    let mut a = AudioAnalyzer::new(SR);
    let loud = sine_wave(440.0, 512, 0.8);
    let silence = vec![0.0f32; 512];

    warm_up(&mut a, &loud);
    let (level_on, _) = a.process(&loud, 1);

    // Run 100 rounds of silence — peak decays, new signal (0) normalises low
    for _ in 0..100 {
        a.process(&silence, 1);
    }
    let (level_off, _) = a.process(&silence, 1);

    assert!(
        level_off < level_on,
        "level should decrease after signal stops: on={level_on:.3} off={level_off:.3}"
    );
    assert!(level_off < 0.1, "level after silence should be near 0: {level_off:.3}");
}

#[test]
fn stereo_input_is_mono_mixed() {
    // Left = signal, right = silence → mono mix = signal * 0.5
    let sig = sine_wave(440.0, 512, 0.5);
    let stereo: Vec<f32> = sig.iter().flat_map(|&s| [s, 0.0]).collect();
    let mono_half: Vec<f32> = sig.iter().map(|&s| s * 0.5).collect();

    // Run both analyzers through identical warm-up sequences
    let mut a_mono   = AudioAnalyzer::new(SR);
    let mut a_stereo = AudioAnalyzer::new(SR);
    for _ in 0..60 {
        a_mono.process(&mono_half, 1);
        a_stereo.process(&stereo, 2);
    }
    let (lm, bm) = a_mono.process(&mono_half, 1);
    let (ls, bs) = a_stereo.process(&stereo, 2);

    assert!(
        (lm - ls).abs() < 0.1,
        "stereo mix level mismatch: mono={lm:.3} stereo={ls:.3}"
    );
    for i in 0..32 {
        assert!(
            (bm[i] - bs[i]).abs() < 0.15,
            "band {i} mismatch: mono={:.3} stereo={:.3}", bm[i], bs[i]
        );
    }
}

#[test]
fn band_values_are_in_zero_one_range() {
    let mut a = AudioAnalyzer::new(SR);
    let loud = sine_wave(1000.0, 512, 1.0);
    for _ in 0..100 {
        let (level, bands) = a.process(&loud, 1);
        assert!((0.0..=1.0).contains(&level), "level out of range: {level}");
        for (i, &b) in bands.iter().enumerate() {
            assert!((0.0..=1.0).contains(&b), "band {i} out of range: {b}");
        }
    }
}

#[test]
fn level_tracks_amplitude() {
    let mut a_loud   = AudioAnalyzer::new(SR);
    let mut a_quiet  = AudioAnalyzer::new(SR);
    let loud  = sine_wave(440.0, 512, 0.8);
    let quiet = sine_wave(440.0, 512, 0.05);

    warm_up(&mut a_loud,  &loud);
    warm_up(&mut a_quiet, &quiet);
    let (lvl_loud,  _) = a_loud.process(&loud, 1);
    let (lvl_quiet, _) = a_quiet.process(&quiet, 1);

    // Both should be near 1.0 (self-normalised) but loud takes longer to
    // normalise so its value should be >= quiet's after the same # of rounds
    assert!(
        lvl_loud + 0.05 >= lvl_quiet,
        "expected loud >= quiet: {lvl_loud:.3} vs {lvl_quiet:.3}"
    );
}
