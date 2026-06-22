use compas_dsp::analysis::{estimate_beatgrid, estimate_tempo, estimate_tempo_diagnostics};

const SR: u32 = 44_100;

fn click_track(bpm: f32, seconds: f32, first_beat_sec: f32) -> Vec<f32> {
    let total = (SR as f32 * seconds) as usize;
    let mut samples = vec![0.0f32; total];
    add_clicks(&mut samples, bpm, first_beat_sec, seconds, 1.0);
    samples
}

fn add_clicks(samples: &mut [f32], bpm: f32, first_beat_sec: f32, until_sec: f32, amp: f32) {
    let period = 60.0 / bpm;
    let mut beat = first_beat_sec;
    while beat < until_sec {
        add_click(samples, beat, amp);
        beat += period;
    }
}

fn add_click(samples: &mut [f32], at_sec: f32, amp: f32) {
    let start = (at_sec * SR as f32).round() as usize;
    for k in 0..64 {
        if let Some(sample) = samples.get_mut(start + k) {
            let decay = 1.0 - k as f32 / 64.0;
            *sample += amp * decay * if k % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
}

fn assert_bpm_close(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.2} BPM +/- {tolerance:.2}, got {actual:.2}"
    );
}

#[test]
fn beat_tracking_handles_common_dance_tempos() {
    for bpm in [90.0, 120.0, 128.0, 150.0] {
        let samples = click_track(bpm, 16.0, 0.0);
        let estimate = estimate_tempo(&samples, SR);
        assert_bpm_close(estimate.bpm, bpm, 2.0);
        assert!(
            estimate.confidence > 0.05,
            "expected non-zero confidence for {bpm} BPM, got {}",
            estimate.confidence
        );
    }
}

#[test]
fn beatgrid_recovers_delayed_first_beat_phase() {
    let samples = click_track(124.0, 16.0, 0.25);
    let grid = estimate_beatgrid(&samples, SR);

    assert_bpm_close(grid.bpm, 124.0, 2.0);
    assert!(
        (grid.first_beat_sec - 0.25).abs() <= 0.04,
        "expected first beat near 0.25 s, got {:.3} s",
        grid.first_beat_sec
    );
    assert!(
        (grid.beat_interval_sec - (60.0 / 124.0)).abs() <= 0.02,
        "wrong beat interval: {:.3}",
        grid.beat_interval_sec
    );
}

#[test]
fn beat_tracking_ignores_sparse_intro_before_steady_section() {
    let mut samples = vec![0.0f32; (SR as f32 * 18.0) as usize];
    add_click(&mut samples, 0.5, 1.0);
    add_click(&mut samples, 2.0, 0.65);
    add_clicks(&mut samples, 128.0, 6.0, 18.0, 1.0);

    let estimate = estimate_tempo(&samples, SR);
    assert_bpm_close(estimate.bpm, 128.0, 2.0);
}

#[test]
#[ignore = "reference case: requires a time-varying tempo model, not the current single-BPM estimator"]
fn beat_tracking_reference_tempo_ramp() {
    let total_sec = 20.0;
    let mut samples = vec![0.0f32; (SR as f32 * total_sec) as usize];
    let mut t = 0.0f32;
    while t < total_sec {
        let progress = t / total_sec;
        let bpm = 118.0 + progress * 8.0;
        add_click(&mut samples, t, 1.0);
        t += 60.0 / bpm;
    }

    let estimate = estimate_tempo(&samples, SR);
    assert_bpm_close(estimate.bpm, 122.0, 3.0);
}

#[test]
#[ignore = "reference case: downbeat weighting is needed to reject half/double tempo traps robustly"]
fn beat_tracking_reference_half_double_tempo_trap() {
    let mut samples = vec![0.0f32; (SR as f32 * 16.0) as usize];
    add_clicks(&mut samples, 64.0, 0.0, 16.0, 1.0);
    add_clicks(&mut samples, 128.0, 60.0 / 128.0, 16.0, 0.35);

    let estimate = estimate_tempo(&samples, SR);
    assert_bpm_close(estimate.bpm, 128.0, 2.0);
}

#[test]
fn diagnostics_expose_half_double_candidates() {
    // Same half/double trap as the ignored selection case: strong 64 BPM on-beats with
    // weaker 128 BPM off-beats. The estimator may still pick the wrong octave, but the
    // diagnostics must make the ambiguity *visible* — that is this slice's whole job.
    let mut samples = vec![0.0f32; (SR as f32 * 16.0) as usize];
    add_clicks(&mut samples, 64.0, 0.0, 16.0, 1.0);
    add_clicks(&mut samples, 128.0, 60.0 / 128.0, 16.0, 0.35);

    let diag = estimate_tempo_diagnostics(&samples, SR).expect("diagnostics for a real signal");

    eprintln!(
        "selected_bpm={:.2} first_beat={:.3} conf={:.3} half={:?} double={:?}",
        diag.selected_bpm,
        diag.first_beat_sec,
        diag.confidence,
        diag.half_tempo_score,
        diag.double_tempo_score
    );
    for (i, c) in diag.candidates.iter().enumerate() {
        eprintln!(
            "  cand[{i}] lag={:.2} raw_bpm={:.2} folded_bpm={:.2} score={:.3}",
            c.lag, c.raw_bpm, c.folded_bpm, c.score
        );
    }

    // Structural invariants the diagnostics must always uphold.
    assert!(
        !diag.candidates.is_empty(),
        "expected at least one candidate"
    );
    assert!(
        diag.selected < diag.candidates.len(),
        "selected index in range"
    );
    assert_eq!(
        diag.candidates[0].score, 1.0,
        "the strongest candidate is peak-relative 1.0"
    );
    assert!(
        (diag.candidates[diag.selected].folded_bpm - diag.selected_bpm).abs() <= 0.5,
        "selected candidate must agree with selected_bpm"
    );

    // The half/double ambiguity is the point: the half-tempo octave (64 BPM, the strong
    // on-beats that fall outside the in-range search) must show strong onset support.
    let half = diag
        .half_tempo_score
        .expect("half-tempo octave should be scorable");
    assert!(
        half > 0.5,
        "half-tempo (64 BPM) support should be visible, got {half:.3}"
    );
}

#[test]
fn confidence_is_lower_for_ambiguous_grids() {
    // A clean, unambiguous click track.
    let clean = click_track(128.0, 16.0, 0.0);
    let clean_conf = estimate_tempo(&clean, SR).confidence;

    // The half/double trap: the estimator's pick has a stronger octave it can't see.
    let mut trap = vec![0.0f32; (SR as f32 * 16.0) as usize];
    add_clicks(&mut trap, 64.0, 0.0, 16.0, 1.0);
    add_clicks(&mut trap, 128.0, 60.0 / 128.0, 16.0, 0.35);
    let trap_conf = estimate_tempo(&trap, SR).confidence;

    // Band-limited-ish noise: no real periodicity.
    let mut state = 0x1234_5678u32;
    let noise: Vec<f32> = (0..(SR as usize * 8))
        .map(|_| {
            // Cheap deterministic LCG noise in [-0.5, 0.5].
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / u32::MAX as f32 - 0.5
        })
        .collect();
    let noise_conf = estimate_tempo(&noise, SR).confidence;

    eprintln!("clean={clean_conf:.3} trap={trap_conf:.3} noise={noise_conf:.3}");

    // Clean fixtures stay usable; the ambiguous grid is demonstrably less trustworthy;
    // and unstructured noise should look untrustworthy.
    assert!(
        clean_conf > 0.1,
        "clean click confidence too low: {clean_conf:.3}"
    );
    assert!(
        trap_conf < clean_conf,
        "ambiguous trap ({trap_conf:.3}) should score below clean ({clean_conf:.3})"
    );
    assert!(
        noise_conf < trap_conf,
        "noise ({noise_conf:.3}) should be the least trustworthy"
    );
}

#[test]
#[ignore = "reference case: swung subdivisions need onset grouping before tempo scoring"]
fn beat_tracking_reference_swung_drums() {
    let mut samples = vec![0.0f32; (SR as f32 * 16.0) as usize];
    let beat = 60.0 / 126.0;
    let mut bar = 0.0f32;
    while bar < 16.0 {
        for step in 0..4 {
            let downbeat = bar + step as f32 * beat;
            add_click(&mut samples, downbeat, 1.0);
            add_click(&mut samples, downbeat + beat * 0.66, 0.35);
        }
        bar += beat * 4.0;
    }

    let estimate = estimate_tempo(&samples, SR);
    assert_bpm_close(estimate.bpm, 126.0, 2.0);
}
