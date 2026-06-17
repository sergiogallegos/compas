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

/// Estimate tempo from mono PCM.
///
/// SCAFFOLD: currently computes the onset (spectral-flux) envelope only and returns
/// a not-yet-implemented confidence of 0. Full autocorrelation tempo lock lands in P1.
pub fn estimate_tempo(samples: &[f32], sample_rate: u32) -> TempoEstimate {
    if samples.is_empty() || sample_rate == 0 {
        return TempoEstimate {
            bpm: 0.0,
            confidence: 0.0,
        };
    }
    let _onset = spectral_flux_envelope(samples, 1024, 512);
    // TODO(P1): autocorrelate `_onset`, pick tempo peak in 70..180 BPM, octave-correct.
    TempoEstimate {
        bpm: 0.0,
        confidence: 0.0,
    }
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
