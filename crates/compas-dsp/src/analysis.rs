//! Offline analysis: BPM/tempo and musical key.
//!
//! NOT real-time-safe — these allocate and run on a worker thread after a local
//! file is decoded. Streaming sources cannot be analyzed here (no PCM); see
//! ARCHITECTURE.md §"Why streaming decks have no beatgrid".
//!
//! The implementations below are deliberately scaffolds with honest, testable
//! behavior and clearly-marked TODOs for Phase 1. The algorithms are chosen so we
//! never need to link GPL code (aubio/Rubber Band are reference-only):
//!   * Tempo: spectral-flux onset envelope → autocorrelation / comb-filter (TODO P1).
//!   * Key:   chromagram → Krumhansl–Schmuckler key profiles (TODO P1).

use rustfft::{num_complex::Complex32, FftPlanner};

/// Result of tempo analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoEstimate {
    pub bpm: f32,
    /// 0..1 rough confidence; lets the UI flag "verify beatgrid".
    pub confidence: f32,
}

/// Result of key analysis (Camelot + traditional notation).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEstimate {
    pub camelot: String,
    pub name: String,
    pub confidence: f32,
}

/// Tempo search range. Results outside are octave-folded into it.
const MIN_BPM: f32 = 70.0;
const MAX_BPM: f32 = 180.0;
const FRAME: usize = 1024;
const HOP: usize = 256;

/// Estimate tempo from mono PCM via autocorrelation of a spectral-flux onset envelope.
///
/// Pipeline: onset envelope (spectral flux) → mean-subtract → autocorrelation over the
/// lag range implied by [`MIN_BPM`, `MAX_BPM`] → pick the peak lag → parabolic
/// interpolation for sub-bin precision → convert lag to BPM.
///
/// This is intentionally a solid-but-simple estimator: no full beat tracking / dynamic
/// programming yet (that, plus downbeat/phase for the beatgrid, is the rest of P1). It
/// uses no GPL code (aubio is reference-only).
pub fn estimate_tempo(samples: &[f32], sample_rate: u32) -> TempoEstimate {
    if samples.is_empty() || sample_rate == 0 {
        return TempoEstimate {
            bpm: 0.0,
            confidence: 0.0,
        };
    }

    let mut env = spectral_flux_envelope(samples, FRAME, HOP);
    if env.len() < 16 {
        return TempoEstimate {
            bpm: 0.0,
            confidence: 0.0,
        };
    }

    // Mean-subtract so the autocorrelation reflects periodicity, not DC energy.
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    for v in env.iter_mut() {
        *v = (*v - mean).max(0.0);
    }

    let env_rate = sample_rate as f32 / HOP as f32; // onset samples per second
    // lag (in env samples) = 60 * env_rate / bpm.
    let lag_min = (60.0 * env_rate / MAX_BPM).floor().max(1.0) as usize;
    let lag_max = ((60.0 * env_rate / MIN_BPM).ceil() as usize).min(env.len() - 1);
    if lag_max <= lag_min {
        return TempoEstimate {
            bpm: 0.0,
            confidence: 0.0,
        };
    }

    let energy: f32 = env.iter().map(|x| x * x).sum::<f32>().max(1e-9);

    let mut best_lag = lag_min;
    let mut best_r = f32::MIN;
    let mut sum_r = 0.0f32;
    let mut count = 0u32;
    for lag in lag_min..=lag_max {
        let mut acc = 0.0f32;
        for i in 0..(env.len() - lag) {
            acc += env[i] * env[i + lag];
        }
        let r = acc / energy;
        sum_r += r;
        count += 1;
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
    }

    // Parabolic interpolation around the integer peak for sub-bin lag precision.
    let refined_lag = parabolic_peak(&env, best_lag, energy).unwrap_or(best_lag as f32);
    let mut bpm = 60.0 * env_rate / refined_lag;

    // Octave-fold into [MIN_BPM, MAX_BPM] (autocorr can lock onto a multiple).
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
    }

    // Confidence: how much the peak stands out above the mean autocorrelation.
    let mean_r = if count > 0 { sum_r / count as f32 } else { 0.0 };
    let confidence = if best_r > 0.0 {
        (1.0 - mean_r / best_r).clamp(0.0, 1.0)
    } else {
        0.0
    };

    TempoEstimate { bpm, confidence }
}

/// Sub-bin lag refinement: fit a parabola to the autocorrelation at `lag-1, lag, lag+1`.
fn parabolic_peak(env: &[f32], lag: usize, energy: f32) -> Option<f32> {
    if lag == 0 || lag + 1 >= env.len() {
        return None;
    }
    let r = |l: usize| -> f32 {
        let mut acc = 0.0f32;
        for i in 0..(env.len() - l) {
            acc += env[i] * env[i + l];
        }
        acc / energy
    };
    let ym1 = r(lag - 1);
    let y0 = r(lag);
    let yp1 = r(lag + 1);
    let denom = ym1 - 2.0 * y0 + yp1;
    if denom.abs() < 1e-12 {
        return Some(lag as f32);
    }
    let delta = 0.5 * (ym1 - yp1) / denom;
    Some(lag as f32 + delta.clamp(-1.0, 1.0))
}

/// Estimate musical key from mono PCM.
///
/// SCAFFOLD: returns "unknown" until the chromagram + K-S correlation lands in P1.
pub fn estimate_key(samples: &[f32], sample_rate: u32) -> KeyEstimate {
    let _ = (samples, sample_rate);
    // TODO(P1): 12-bin chromagram via constant-Q-ish mapping of FFT bins, then
    // correlate against Krumhansl–Schmuckler major/minor profiles.
    KeyEstimate {
        camelot: "—".to_string(),
        name: "unknown".to_string(),
        confidence: 0.0,
    }
}

/// Spectral-flux onset detection envelope: sum of positive bin-to-bin magnitude
/// changes per hop. The basis for tempo estimation. Offline (allocates).
fn spectral_flux_envelope(samples: &[f32], frame: usize, hop: usize) -> Vec<f32> {
    if samples.len() < frame || frame == 0 || hop == 0 {
        return Vec::new();
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame);

    let window: Vec<f32> = (0..frame)
        .map(|n| {
            // Hann window.
            let x = (std::f32::consts::PI * n as f32 / (frame as f32 - 1.0)).sin();
            x * x
        })
        .collect();

    let mut buf = vec![Complex32::new(0.0, 0.0); frame];
    let mut prev_mag = vec![0.0f32; frame / 2 + 1];
    let mut envelope = Vec::with_capacity(samples.len() / hop);

    let mut pos = 0;
    while pos + frame <= samples.len() {
        for i in 0..frame {
            buf[i] = Complex32::new(samples[pos + i] * window[i], 0.0);
        }
        fft.process(&mut buf);

        let mut flux = 0.0f32;
        for (k, prev) in prev_mag.iter_mut().enumerate() {
            let mag = buf[k].norm();
            let diff = mag - *prev;
            if diff > 0.0 {
                flux += diff;
            }
            *prev = mag;
        }
        envelope.push(flux);
        pos += hop;
    }
    envelope
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_on_empty_is_zero_confidence() {
        let est = estimate_tempo(&[], 44_100);
        assert_eq!(est.confidence, 0.0);
    }

    #[test]
    fn tempo_on_synthetic_click_track() {
        // 120 BPM = 2 beats/sec -> a click every 0.5 s.
        let sr = 44_100u32;
        let period = (sr as f32 * 0.5) as usize; // 22050 samples
        let total = sr as usize * 12; // 12 seconds
        let mut samples = vec![0.0f32; total];
        let mut t = 0;
        while t < total {
            // Short decaying click so it has broadband onset energy.
            for k in 0..64 {
                if t + k < total {
                    samples[t + k] = (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
                }
            }
            t += period;
        }
        let est = estimate_tempo(&samples, sr);
        assert!(
            (est.bpm - 120.0).abs() <= 2.0,
            "expected ~120 BPM, got {}",
            est.bpm
        );
        assert!(est.confidence > 0.0);
    }

    #[test]
    fn spectral_flux_has_expected_frame_count() {
        let samples = vec![0.1f32; 4096];
        let env = spectral_flux_envelope(&samples, 1024, 512);
        // (4096 - 1024) / 512 + 1 = 7 frames.
        assert_eq!(env.len(), 7);
    }

    #[test]
    fn key_scaffold_is_unknown() {
        let est = estimate_key(&[0.0; 1024], 44_100);
        assert_eq!(est.name, "unknown");
    }
}
