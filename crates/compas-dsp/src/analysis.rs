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

/// Tempo plus the phase of beat one — enough to draw a beatgrid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatGrid {
    pub bpm: f32,
    /// Time of the first beat, in seconds from the start of the track.
    pub first_beat_sec: f32,
    /// Seconds between beats (60 / bpm), cached for the UI.
    pub beat_interval_sec: f32,
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
/// Shared core: onset envelope → autocorrelation tempo. Returns the (mean-subtracted)
/// envelope, its sample rate, the period in env-samples, the BPM, and confidence.
fn tempo_core(samples: &[f32], sample_rate: u32) -> Option<(Vec<f32>, f32, f32, f32, f32)> {
    if samples.is_empty() || sample_rate == 0 {
        return None;
    }
    let mut env = spectral_flux_envelope(samples, FRAME, HOP);
    if env.len() < 16 {
        return None;
    }
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    for v in env.iter_mut() {
        *v = (*v - mean).max(0.0);
    }

    let env_rate = sample_rate as f32 / HOP as f32;
    let lag_min = (60.0 * env_rate / MAX_BPM).floor().max(1.0) as usize;
    let lag_max = ((60.0 * env_rate / MIN_BPM).ceil() as usize).min(env.len() - 1);
    if lag_max <= lag_min {
        return None;
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

    let refined_lag = parabolic_peak(&env, best_lag, energy).unwrap_or(best_lag as f32);
    let mut bpm = 60.0 * env_rate / refined_lag;
    let mut period = refined_lag;
    while bpm < MIN_BPM {
        bpm *= 2.0;
        period *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
        period /= 2.0;
    }

    let mean_r = if count > 0 { sum_r / count as f32 } else { 0.0 };
    let confidence = if best_r > 0.0 {
        (1.0 - mean_r / best_r).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Some((env, env_rate, period, bpm, confidence))
}

/// Estimate tempo only (BPM + confidence).
pub fn estimate_tempo(samples: &[f32], sample_rate: u32) -> TempoEstimate {
    match tempo_core(samples, sample_rate) {
        Some((_, _, _, bpm, confidence)) => TempoEstimate { bpm, confidence },
        None => TempoEstimate {
            bpm: 0.0,
            confidence: 0.0,
        },
    }
}

/// Estimate a full beatgrid: tempo plus the phase (time) of the first beat, found by
/// combing a beat-spaced pulse train across the onset envelope and taking the offset
/// with the most onset energy.
pub fn estimate_beatgrid(samples: &[f32], sample_rate: u32) -> BeatGrid {
    let Some((env, _env_rate, period, bpm, confidence)) = tempo_core(samples, sample_rate) else {
        return BeatGrid {
            bpm: 0.0,
            first_beat_sec: 0.0,
            beat_interval_sec: 0.0,
            confidence: 0.0,
        };
    };

    // Comb over candidate phases [0, period) in env-samples; pick max accumulated energy.
    let mut best_phi = 0usize;
    let mut best_score = f32::MIN;
    let phi_max = period.ceil() as usize;
    for phi in 0..phi_max {
        let mut score = 0.0f32;
        let mut idx = phi as f32;
        while (idx as usize) < env.len() {
            score += env[idx as usize];
            idx += period;
        }
        if score > best_score {
            best_score = score;
            best_phi = phi;
        }
    }

    // env-sample → seconds: each env sample spans HOP/sample_rate seconds.
    let sec_per_env = HOP as f32 / sample_rate as f32;
    BeatGrid {
        bpm,
        first_beat_sec: best_phi as f32 * sec_per_env,
        beat_interval_sec: if bpm > 0.0 { 60.0 / bpm } else { 0.0 },
        confidence,
    }
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

// Krumhansl–Kessler key profiles (major / minor), indexed from the tonic.
const KS_MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const KS_MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
const PITCH_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
// Camelot codes indexed by pitch class (C=0 .. B=11).
const CAMELOT_MAJOR: [&str; 12] = [
    "8B", "3B", "10B", "5B", "12B", "7B", "2B", "9B", "4B", "11B", "6B", "1B",
];
const CAMELOT_MINOR: [&str; 12] = [
    "5A", "12A", "7A", "2A", "9A", "4A", "11A", "6A", "1A", "8A", "3A", "10A",
];

/// Estimate musical key by building a 12-bin chromagram and correlating it against the
/// Krumhansl–Schmuckler major/minor profiles rotated to all 12 tonics (24 candidates).
pub fn estimate_key(samples: &[f32], sample_rate: u32) -> KeyEstimate {
    let unknown = || KeyEstimate {
        camelot: "—".to_string(),
        name: "—".to_string(),
        confidence: 0.0,
    };
    if samples.is_empty() || sample_rate == 0 {
        return unknown();
    }
    let chroma = chromagram(samples, sample_rate);
    if chroma.iter().sum::<f32>() <= f32::EPSILON {
        return unknown();
    }

    let mut best_tonic = 0usize;
    let mut best_major = true;
    let mut best_r = f32::MIN;
    for t in 0..12 {
        let rmaj = pearson_rotated(&chroma, &KS_MAJOR, t);
        if rmaj > best_r {
            best_r = rmaj;
            best_tonic = t;
            best_major = true;
        }
        let rmin = pearson_rotated(&chroma, &KS_MINOR, t);
        if rmin > best_r {
            best_r = rmin;
            best_tonic = t;
            best_major = false;
        }
    }

    let (camelot, name) = if best_major {
        (
            CAMELOT_MAJOR[best_tonic],
            PITCH_NAMES[best_tonic].to_string(),
        )
    } else {
        (
            CAMELOT_MINOR[best_tonic],
            format!("{}m", PITCH_NAMES[best_tonic]),
        )
    };
    KeyEstimate {
        camelot: camelot.to_string(),
        name,
        confidence: best_r.clamp(0.0, 1.0),
    }
}

/// Accumulate a 12-bin chromagram (energy per pitch class) over the signal.
fn chromagram(samples: &[f32], sample_rate: u32) -> [f32; 12] {
    const FRAME: usize = 4096;
    const HOP: usize = 2048;
    let mut chroma = [0.0f32; 12];
    if samples.len() < FRAME {
        return chroma;
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|n| {
            let x = (std::f32::consts::PI * n as f32 / (FRAME as f32 - 1.0)).sin();
            x * x
        })
        .collect();
    let mut buf = vec![Complex32::new(0.0, 0.0); FRAME];

    let mut pos = 0;
    while pos + FRAME <= samples.len() {
        for i in 0..FRAME {
            buf[i] = Complex32::new(samples[pos + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        for (k, item) in buf.iter().enumerate().take(FRAME / 2).skip(1) {
            let freq = k as f32 * sample_rate as f32 / FRAME as f32;
            if !(55.0..=2000.0).contains(&freq) {
                continue;
            }
            let midi = 69.0 + 12.0 * (freq / 440.0).log2();
            let pc = (((midi.round() as i32) % 12) + 12) % 12;
            chroma[pc as usize] += item.norm();
        }
        pos += HOP;
    }
    chroma
}

/// Pearson correlation between the chroma vector and a key profile rotated so its tonic
/// sits at pitch class `t`.
fn pearson_rotated(chroma: &[f32; 12], profile: &[f32; 12], t: usize) -> f32 {
    let mut rp = [0.0f32; 12];
    for i in 0..12 {
        rp[i] = profile[(i + 12 - t) % 12];
    }
    let mc = chroma.iter().sum::<f32>() / 12.0;
    let mp = rp.iter().sum::<f32>() / 12.0;
    let mut cov = 0.0;
    let mut vc = 0.0;
    let mut vp = 0.0;
    for i in 0..12 {
        let dc = chroma[i] - mc;
        let dp = rp[i] - mp;
        cov += dc * dp;
        vc += dc * dc;
        vp += dp * dp;
    }
    let denom = (vc * vp).sqrt();
    if denom < 1e-9 {
        0.0
    } else {
        cov / denom
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

/// Linear gain that brings `samples` to a reference loudness (~−18 dBFS RMS — the classic
/// ReplayGain target), for auto-gain/loudness normalization across a library. Accepts interleaved
/// stereo or mono. Clamped to a musical range so silent or hot tracks don't jump. Offline; not
/// RT-safe (iterates the whole buffer).
pub fn replaygain_linear(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 1.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    if rms < 1e-6 {
        return 1.0; // silence — leave it alone
    }
    const TARGET_RMS: f64 = 0.125; // ≈ −18 dBFS
    ((TARGET_RMS / rms) as f32).clamp(0.25, 4.0)
}

/// Per-waveform-bin energy split into three frequency bands (low / mid / high), for drawing
/// frequency-colored waveforms (low→red, mid→green, high→blue). `samples` is interleaved stereo;
/// each output entry is `[low, mid, high]` RMS for one `bin_frames`-sized bin, globally normalized
/// to `0..=1` by the largest single-band value so colors are comparable across the track. Offline.
pub fn band_peaks(samples: &[f32], sample_rate: u32, bin_frames: usize) -> Vec<[f32; 3]> {
    let frames = samples.len() / 2;
    if frames == 0 || bin_frames == 0 || sample_rate == 0 {
        return Vec::new();
    }
    let sr = sample_rate as f32;
    // Three bands: low (<250 Hz), mid (250 Hz–2.5 kHz, an HPF→LPF cascade), high (>2.5 kHz).
    let mut low = crate::Biquad::new(crate::BiquadCoeffs::low_pass(250.0, sr, 0.707));
    let mut mid_hp = crate::Biquad::new(crate::BiquadCoeffs::high_pass(250.0, sr, 0.707));
    let mut mid_lp = crate::Biquad::new(crate::BiquadCoeffs::low_pass(2500.0, sr, 0.707));
    let mut high = crate::Biquad::new(crate::BiquadCoeffs::high_pass(2500.0, sr, 0.707));

    let nbins = frames.div_ceil(bin_frames);
    let mut sums = vec![[0.0f64; 3]; nbins];
    let mut counts = vec![0u32; nbins];

    for i in 0..frames {
        let mono = 0.5 * (samples[i * 2] + samples[i * 2 + 1]);
        let l = low.process(mono);
        let m = mid_lp.process(mid_hp.process(mono));
        let h = high.process(mono);
        let bin = i / bin_frames;
        sums[bin][0] += (l as f64) * (l as f64);
        sums[bin][1] += (m as f64) * (m as f64);
        sums[bin][2] += (h as f64) * (h as f64);
        counts[bin] += 1;
    }

    let mut out = vec![[0.0f32; 3]; nbins];
    let mut peak = 0.0f32;
    for (bin, (s, &c)) in sums.iter().zip(counts.iter()).enumerate() {
        if c == 0 {
            continue;
        }
        for b in 0..3 {
            let rms = (s[b] / c as f64).sqrt() as f32;
            out[bin][b] = rms;
            peak = peak.max(rms);
        }
    }
    if peak > 1e-9 {
        for bin in out.iter_mut() {
            for b in bin.iter_mut() {
                *b /= peak;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_peaks_route_energy_by_frequency() {
        let sr = 44_100u32;
        let secs = 1.0;
        let n = (sr as f32 * secs) as usize;
        let sine = |hz: f32| -> Vec<f32> {
            (0..n)
                .flat_map(|i| {
                    let s = (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin() * 0.5;
                    [s, s]
                })
                .collect()
        };
        let bins_low = band_peaks(&sine(60.0), sr, 4096);
        let avg = |v: &[[f32; 3]], b: usize| v.iter().map(|x| x[b]).sum::<f32>() / v.len() as f32;
        // A 60 Hz tone lands mostly in the low band.
        assert!(
            avg(&bins_low, 0) > avg(&bins_low, 2),
            "low tone should be low-band heavy"
        );
        let bins_high = band_peaks(&sine(8000.0), sr, 4096);
        assert!(
            avg(&bins_high, 2) > avg(&bins_high, 0),
            "high tone should be high-band heavy"
        );
        assert!(band_peaks(&[], sr, 4096).is_empty());
    }

    #[test]
    fn replaygain_boosts_quiet_and_attenuates_loud() {
        // RMS ≈ 0.0625 (half target) → boost ~2x.
        let quiet: Vec<f32> = (0..10_000)
            .map(|i| if i % 2 == 0 { 0.0625 } else { -0.0625 })
            .collect();
        let g = replaygain_linear(&quiet);
        assert!((g - 2.0).abs() < 0.1, "quiet boost was {g}");
        // Full-scale square → RMS 1.0 → attenuate toward target.
        let loud: Vec<f32> = (0..10_000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!(replaygain_linear(&loud) < 1.0, "loud track attenuated");
        // Silence and empty are left unchanged.
        assert_eq!(replaygain_linear(&[0.0; 1000]), 1.0);
        assert_eq!(replaygain_linear(&[]), 1.0);
    }

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
    fn beatgrid_on_synthetic_click_track() {
        let sr = 44_100u32;
        let period = (sr as f32 * 0.5) as usize; // 120 BPM
        let total = sr as usize * 12;
        let mut samples = vec![0.0f32; total];
        let mut t = 0;
        while t < total {
            for k in 0..64 {
                if t + k < total {
                    samples[t + k] = (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
                }
            }
            t += period;
        }
        let grid = estimate_beatgrid(&samples, sr);
        assert!(
            (grid.beat_interval_sec - 0.5).abs() < 0.02,
            "interval {}",
            grid.beat_interval_sec
        );
        // clicks start at t=0, so the first beat phase should be near 0 (mod a beat).
        assert!(
            grid.first_beat_sec < 0.06 || (0.5 - grid.first_beat_sec) < 0.06,
            "first beat {}",
            grid.first_beat_sec
        );
    }

    #[test]
    fn key_unknown_on_silence() {
        assert_eq!(estimate_key(&[], 44_100).name, "—");
        assert_eq!(estimate_key(&[0.0; 8192], 44_100).name, "—");
    }

    #[test]
    fn key_detects_a_plausible_key_for_a_major_triad() {
        // C-major triad (C4/E4/G4) sustained — expect a confident, valid key code.
        let sr = 44_100u32;
        let n = sr as usize * 3;
        let freqs = [261.63, 329.63, 392.0];
        let mut s = vec![0.0f32; n];
        for (i, sample) in s.iter_mut().enumerate() {
            let t = i as f32 / sr as f32;
            *sample = freqs
                .iter()
                .map(|f| (2.0 * std::f32::consts::PI * f * t).sin())
                .sum::<f32>()
                / 3.0;
        }
        let est = estimate_key(&s, sr);
        assert_ne!(est.camelot, "—");
        assert!(est.confidence > 0.3, "low confidence {}", est.confidence);
    }
}
