//! Streaming-chunk harness for the causal live beat tracker (adoption-plan slice 5).
//!
//! Every test feeds the tracker fixed-size chunks with no look-ahead and asserts convergence:
//! cold start, tempo step, dropout, false-onset robustness, and the no-look-ahead invariant
//! (chunk boundaries must not change the result). See `docs/research/live-input-beat-tracking.md`.

use compas_dsp::{LiveEstimate, LiveTracker};

const SR: u32 = 44_100;

/// Mono click track: a short decaying broadband click on each beat from `first` to `secs`.
fn clicks(bpm: f32, secs: f32, first: f32) -> Vec<f32> {
    let n = (SR as f32 * secs) as usize;
    let mut s = vec![0.0f32; n];
    let period = 60.0 / bpm;
    let mut t = first;
    while t < secs {
        let start = (t * SR as f32) as usize;
        for k in 0..64usize {
            if let Some(x) = s.get_mut(start + k) {
                *x += (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        t += period;
    }
    s
}

/// Deterministic LCG noise in [-amp, amp], added in place — for false-onset robustness.
fn add_noise(s: &mut [f32], amp: f32) {
    let mut state = 0x1234_5678u32;
    for x in s.iter_mut() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *x += ((state >> 8) as f32 / u32::MAX as f32 - 0.5) * 2.0 * amp;
    }
}

/// Feed `sig` to a fresh tracker in `chunk`-sized pieces; return the final estimate.
fn run_chunked(sig: &[f32], chunk: usize) -> Option<LiveEstimate> {
    let mut t = LiveTracker::new(SR);
    let mut last = None;
    for c in sig.chunks(chunk) {
        if let Some(e) = t.push(c) {
            last = Some(e);
        }
    }
    last
}

fn assert_bpm(actual: f32, expected: f32, tol: f32) {
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected:.1} BPM ±{tol:.1}, got {actual:.2}"
    );
}

#[test]
fn cold_start_locks_to_steady_tempo() {
    let est = run_chunked(&clicks(120.0, 12.0, 0.0), 1024).expect("an estimate after 12 s");
    assert_bpm(est.bpm, 120.0, 3.0);
    assert!(
        est.locked,
        "should lock onto a steady click (conf {:.3})",
        est.confidence
    );
    assert!(
        est.beat_phase >= 0.0 && est.beat_phase < 1.0,
        "phase in range"
    );
}

#[test]
fn locks_to_a_faster_tempo() {
    let est = run_chunked(&clicks(140.0, 12.0, 0.0), 1024).expect("estimate");
    assert_bpm(est.bpm, 140.0, 3.0);
    assert!(est.locked);
}

#[test]
fn relocks_after_a_tempo_step() {
    // 120 for 8 s, then 140 for 8 s. After a full window of 140 the estimate should follow.
    let mut sig = clicks(120.0, 8.0, 0.0);
    sig.extend(clicks(140.0, 10.0, 0.0));
    let est = run_chunked(&sig, 1024).expect("estimate");
    assert_bpm(est.bpm, 140.0, 4.0);
}

#[test]
fn holds_tempo_through_a_dropout() {
    // Clicks, 2 s of silence, then clicks again — tempo should survive the gap.
    let mut sig = clicks(120.0, 6.0, 0.0);
    sig.extend(std::iter::repeat_n(0.0, (SR as f32 * 2.0) as usize));
    sig.extend(clicks(120.0, 6.0, 0.0));
    let est = run_chunked(&sig, 1024).expect("estimate");
    assert_bpm(est.bpm, 120.0, 3.0);
}

#[test]
fn false_onset_bursts_do_not_break_lock() {
    let mut sig = clicks(120.0, 12.0, 0.0);
    add_noise(&mut sig, 0.15); // well below the click amplitude
    let est = run_chunked(&sig, 1024).expect("estimate");
    assert_bpm(est.bpm, 120.0, 3.0);
    assert!(
        est.locked,
        "noise must not unlock a clear pulse (conf {:.3})",
        est.confidence
    );
}

#[test]
fn chunking_is_deterministic_no_lookahead() {
    // The no-look-ahead invariant: feeding the whole signal at once must give the exact same
    // result as feeding it in arbitrary chunks (partial hops are buffered across calls).
    let sig = clicks(128.0, 10.0, 0.0);

    let mut whole = LiveTracker::new(SR);
    whole.push(&sig);
    let a = whole.estimate().expect("estimate");

    let b = run_chunked(&sig, 333).expect("estimate"); // deliberately not a multiple of HOP

    assert_eq!(a.bpm, b.bpm, "bpm differs across chunking");
    assert_eq!(a.beat_phase, b.beat_phase, "phase differs across chunking");
    assert_eq!(
        a.confidence, b.confidence,
        "confidence differs across chunking"
    );
    assert_eq!(a.locked, b.locked, "lock differs across chunking");
}

#[test]
fn silence_does_not_lock() {
    // Pure silence has no periodicity → never reports a confident lock.
    let est = run_chunked(&vec![0.0f32; (SR as f32 * 8.0) as usize], 1024);
    if let Some(e) = est {
        assert!(
            !e.locked,
            "silence must not lock (conf {:.3})",
            e.confidence
        );
    }
}
