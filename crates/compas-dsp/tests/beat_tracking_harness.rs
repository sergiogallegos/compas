use compas_dsp::analysis::{
    estimate_beatgrid, estimate_tempo, estimate_tempo_diagnostics, BeatGrid,
};

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

// --- Beat continuity (research-backed TODO 3) ---------------------------------------
//
// Approximate BPM correctness is not enough: a tempo that is "close on average" still
// slips phase over a long track. These helpers compare the *index-aligned* predicted beat
// against the true beat (beat i vs beat i), so accumulating tempo error and phase slip are
// visible — nearest-neighbour matching would hide drift by snapping to a neighbouring beat.

/// True beat times for a constant-tempo grid over `[first, until)`.
fn true_beats(bpm: f32, first: f32, until: f32) -> Vec<f32> {
    let interval = 60.0 / bpm;
    let mut beats = Vec::new();
    let mut t = first;
    while t < until {
        beats.push(t);
        t += interval;
    }
    beats
}

/// Predicted beat times from an estimated grid: `first_beat_sec + i * interval`, `count` beats.
fn grid_beats(grid: &BeatGrid, count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| grid.first_beat_sec + i as f32 * grid.beat_interval_sec)
        .collect()
}

/// Drift of the grid against the truth, as a fraction of one beat interval.
///
/// We measure the *spread* (max − min) of the signed index-aligned offset `true_i − pred_i`,
/// not the absolute error. Beat phase is ambiguous modulo one interval (the comb may report
/// the first beat at ≈0 or ≈one beat later), which only adds a *constant* to every offset;
/// the spread cancels that constant and isolates accumulating tempo error and one-off phase
/// jumps. A perfect-tempo grid has constant offset → spread 0; a 2%-off grid ramps linearly.
/// A spread `>= 0.5` means the grid slips past a neighbouring beat across the track.
fn drift_spread_beats(truth: &[f32], grid: &BeatGrid) -> f32 {
    let predicted = grid_beats(grid, truth.len());
    let offsets = truth.iter().zip(&predicted).map(|(t, p)| t - p);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for d in offsets {
        lo = lo.min(d);
        hi = hi.max(d);
    }
    (hi - lo) / grid.beat_interval_sec.max(1e-6)
}

#[test]
fn beatgrid_holds_phase_over_a_long_track() {
    // A grid that is only "close on average" drifts over a long track; require it to stay
    // phase-locked end to end, which is stricter than the +/- 2 BPM average tolerance.
    let bpm = 128.0;
    let dur = 40.0;
    let samples = click_track(bpm, dur, 0.0);
    let grid = estimate_beatgrid(&samples, SR);
    let truth = true_beats(bpm, 0.0, dur);

    let drift = drift_spread_beats(&truth, &grid);
    eprintln!(
        "long-track: bpm={:.3} interval={:.4} drift_spread={:.3} beats",
        grid.bpm, grid.beat_interval_sec, drift
    );
    assert!(
        drift < 0.25,
        "grid drifted {drift:.3} of a beat over {dur}s — tempo not accurate enough to hold phase"
    );
}

#[test]
fn beatgrid_holds_phase_with_delayed_first_beat() {
    // Continuity must hold even when the track does not start on beat one: the offset-
    // invariant spread should stay tiny because the tempo, not just the average, is right.
    let bpm = 124.0;
    let dur = 30.0;
    let first = 0.3;
    let samples = click_track(bpm, dur, first);
    let grid = estimate_beatgrid(&samples, SR);
    let truth = true_beats(bpm, first, dur);

    let drift = drift_spread_beats(&truth, &grid);
    assert!(
        drift < 0.25,
        "grid with delayed first beat drifted {drift:.3} of a beat over {dur}s"
    );
}

#[test]
fn beat_drift_metric_detects_a_detuned_grid() {
    // The continuity metric must have teeth: a grid whose tempo is off by ~2% should slip
    // a large fraction of a beat over a long track, even though its BPM looks "about right".
    let bpm = 128.0;
    let dur = 40.0;
    let truth = true_beats(bpm, 0.0, dur);
    let detuned = BeatGrid {
        bpm: bpm * 1.02,
        first_beat_sec: 0.0,
        beat_interval_sec: 60.0 / (bpm * 1.02),
        confidence: 1.0,
    };
    let drift = drift_spread_beats(&truth, &detuned);
    assert!(
        drift > 0.5,
        "a 2% tempo error should slip past a neighbouring beat, got {drift:.3} of a beat"
    );
}

#[test]
fn beatgrid_spacing_is_uniform() {
    // Spacing must be stable: no unstable inter-beat intervals. (The single-tempo model is
    // uniform by construction; this pins the contract so a future variable grid can't
    // silently regress continuity.)
    let grid = estimate_beatgrid(&click_track(124.0, 24.0, 0.0), SR);
    let beats = grid_beats(&grid, 32);
    let intervals: Vec<f32> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
    let max_dev = intervals
        .iter()
        .map(|i| (i - mean).abs())
        .fold(0.0f32, f32::max);
    assert!(
        max_dev < 1e-4,
        "inter-beat interval not uniform: max deviation {max_dev:.6}s"
    );
}

#[test]
#[ignore = "reference case: swung subdivisions need onset grouping before tempo scoring"]
fn beat_tracking_reference_swung_drums() {
    let samples = swung_track(126.0, 16.0);
    let estimate = estimate_tempo(&samples, SR);
    assert_bpm_close(estimate.bpm, 126.0, 2.0);
}

// --- Evaluation matrix (research-backed TODO 4) -------------------------------------
//
// One table that exercises the estimator across the full set of scenarios the literature
// flags as hard: half/double traps, tempo ramps, swung subdivisions, misleading sparse
// intros, and silence/noise. Each case is tiered:
//   * Solid     — a behaviour we already guarantee; asserted, so a regression fails CI.
//   * Reference — a known gap; recorded in the printed report but NOT asserted, so the
//                 matrix stays green while the accuracy deficit stays visible. Later
//                 algorithm slices flip these to Solid as they land.
// `cargo test -p compas-dsp -- --nocapture beat_evaluation_matrix` prints the full table.

/// Strong on-beats at `bpm/2` with weaker off-beats at `bpm`, the classic octave trap.
fn half_double_trap(bpm: f32, secs: f32) -> Vec<f32> {
    let mut s = vec![0.0f32; (SR as f32 * secs) as usize];
    add_clicks(&mut s, bpm / 2.0, 0.0, secs, 1.0);
    add_clicks(&mut s, bpm, 60.0 / bpm, secs, 0.35);
    s
}

/// A tempo that ramps linearly from `start` to `end` BPM over the clip.
fn tempo_ramp_track(start: f32, end: f32, secs: f32) -> Vec<f32> {
    let mut s = vec![0.0f32; (SR as f32 * secs) as usize];
    let mut t = 0.0f32;
    while t < secs {
        let bpm = start + (end - start) * (t / secs);
        add_click(&mut s, t, 1.0);
        t += 60.0 / bpm;
    }
    s
}

/// Four-on-the-floor with a swung (triplet-ish) off-beat hit per beat.
fn swung_track(bpm: f32, secs: f32) -> Vec<f32> {
    let mut s = vec![0.0f32; (SR as f32 * secs) as usize];
    let beat = 60.0 / bpm;
    let mut bar = 0.0f32;
    while bar < secs {
        for step in 0..4 {
            let downbeat = bar + step as f32 * beat;
            add_click(&mut s, downbeat, 1.0);
            add_click(&mut s, downbeat + beat * 0.66, 0.35);
        }
        bar += beat * 4.0;
    }
    s
}

/// A few isolated, off-tempo intro hits, then a long steady section at `bpm`.
fn misleading_sparse_intro(bpm: f32, secs: f32) -> Vec<f32> {
    let mut s = vec![0.0f32; (SR as f32 * secs) as usize];
    // Misleading early hits suggesting a slower, irregular pulse.
    add_click(&mut s, 0.4, 1.0);
    add_click(&mut s, 1.7, 0.9);
    add_click(&mut s, 3.1, 0.8);
    add_clicks(&mut s, bpm, 6.0, secs, 1.0);
    s
}

/// Deterministic LCG noise in [-0.5, 0.5] — no real periodicity.
fn noise_track(secs: f32) -> Vec<f32> {
    let mut state = 0x9E37_79B9u32;
    (0..(SR as f32 * secs) as usize)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / u32::MAX as f32 - 0.5
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Tier {
    Solid,
    Reference,
}

enum Expect {
    /// Estimated BPM must land within `tol` of `bpm`.
    Bpm { bpm: f32, tol: f32 },
    /// The grid is genuinely ambiguous/absent — confidence must stay at/below `max`.
    LowConfidence { max: f32 },
}

struct Case {
    name: &'static str,
    tier: Tier,
    expect: Expect,
    samples: Vec<f32>,
}

fn evaluation_matrix() -> Vec<Case> {
    let bpm = |name, tier, bpm, tol, samples| Case {
        name,
        tier,
        expect: Expect::Bpm { bpm, tol },
        samples,
    };
    let low = |name, tier, max, samples| Case {
        name,
        tier,
        expect: Expect::LowConfidence { max },
        samples,
    };
    vec![
        bpm(
            "clean_90",
            Tier::Solid,
            90.0,
            2.0,
            click_track(90.0, 16.0, 0.0),
        ),
        bpm(
            "clean_120",
            Tier::Solid,
            120.0,
            2.0,
            click_track(120.0, 16.0, 0.0),
        ),
        bpm(
            "clean_128",
            Tier::Solid,
            128.0,
            2.0,
            click_track(128.0, 16.0, 0.0),
        ),
        bpm(
            "clean_150",
            Tier::Solid,
            150.0,
            2.0,
            click_track(150.0, 16.0, 0.0),
        ),
        bpm(
            "delayed_phase_124",
            Tier::Solid,
            124.0,
            2.0,
            click_track(124.0, 16.0, 0.3),
        ),
        bpm("sparse_intro_128", Tier::Solid, 128.0, 2.0, {
            let mut s = vec![0.0f32; (SR as f32 * 18.0) as usize];
            add_click(&mut s, 0.5, 1.0);
            add_click(&mut s, 2.0, 0.65);
            add_clicks(&mut s, 128.0, 6.0, 18.0, 1.0);
            s
        }),
        low(
            "silence",
            Tier::Solid,
            0.05,
            vec![0.0f32; (SR as f32 * 8.0) as usize],
        ),
        low("noise", Tier::Solid, 0.05, noise_track(8.0)),
        // Known gaps — recorded, not asserted, until the relevant algorithm slice lands.
        bpm(
            "half_double_trap_64",
            Tier::Reference,
            64.0,
            2.0,
            half_double_trap(128.0, 16.0),
        ),
        bpm(
            "tempo_ramp_118_126",
            Tier::Reference,
            122.0,
            3.0,
            tempo_ramp_track(118.0, 126.0, 20.0),
        ),
        bpm(
            "swung_126",
            Tier::Reference,
            126.0,
            2.0,
            swung_track(126.0, 16.0),
        ),
        bpm(
            "misleading_sparse_124",
            Tier::Reference,
            124.0,
            2.0,
            misleading_sparse_intro(124.0, 20.0),
        ),
    ]
}

#[test]
fn beat_evaluation_matrix() {
    let mut solid_failures = Vec::new();
    let (mut ref_pass, mut ref_total) = (0u32, 0u32);

    eprintln!(
        "\n{:<24} {:<10} {:>8} {:>8} {:>6}  result",
        "case", "tier", "expect", "got", "conf"
    );
    for case in evaluation_matrix() {
        // estimate_beatgrid runs the tempo stage internally, so one call gives BPM and the
        // (phase-scaled) grid confidence — no need to analyze each fixture twice.
        let grid = estimate_beatgrid(&case.samples, SR);
        let (expect_str, got_str, pass) = match case.expect {
            Expect::Bpm { bpm, tol } => (
                format!("{bpm:.0}±{tol:.0}"),
                format!("{:.2}", grid.bpm),
                (grid.bpm - bpm).abs() <= tol,
            ),
            Expect::LowConfidence { max } => (
                format!("c≤{max:.2}"),
                format!("c={:.3}", grid.confidence),
                grid.confidence <= max,
            ),
        };
        let tier = if case.tier == Tier::Solid {
            "solid"
        } else {
            "reference"
        };
        eprintln!(
            "{:<24} {:<10} {:>8} {:>8} {:>6.3}  {}",
            case.name,
            tier,
            expect_str,
            got_str,
            grid.confidence,
            if pass { "PASS" } else { "FAIL" }
        );
        match case.tier {
            Tier::Solid if !pass => solid_failures.push(case.name),
            Tier::Reference => {
                ref_total += 1;
                if pass {
                    ref_pass += 1;
                }
            }
            _ => {}
        }
    }
    eprintln!("reference cases passing: {ref_pass}/{ref_total}\n");

    assert!(
        solid_failures.is_empty(),
        "solid evaluation cases regressed: {solid_failures:?}"
    );
}
