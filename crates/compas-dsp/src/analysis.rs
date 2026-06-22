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

/// One tempo candidate the autocorrelation stage considered, for diagnostics only.
///
/// These are exposed by [`estimate_tempo_diagnostics`] so half/double decisions and
/// competing phases can be *seen* before we change tempo-selection behavior. They do
/// not affect [`estimate_tempo`] / [`estimate_beatgrid`] output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoCandidate {
    /// Autocorrelation lag in onset-envelope samples (sub-sample refined).
    pub lag: f32,
    /// BPM implied by `lag` before octave folding into [`MIN_BPM`, `MAX_BPM`].
    pub raw_bpm: f32,
    /// BPM after octave folding — directly comparable to [`TempoEstimate::bpm`].
    pub folded_bpm: f32,
    /// Autocorrelation score relative to the winning peak (`1.0` == the selected peak).
    pub score: f32,
}

/// Debug/diagnostic view into tempo selection: the ranked candidates the estimator
/// considered, which one it chose, the resulting beat phase, and how much onset support
/// the half-tempo and double-tempo octaves have.
///
/// This is intentionally additive — it does **not** change the public app contract
/// ([`TempoEstimate`] / [`BeatGrid`]). It exists so ambiguous beatgrid decisions are
/// visible to tooling and tests (adoption-plan slice 1) before selection behavior changes.
#[derive(Debug, Clone, PartialEq)]
pub struct TempoDiagnostics {
    /// In-range autocorrelation peaks, strongest first (capped at [`MAX_CANDIDATES`]).
    pub candidates: Vec<TempoCandidate>,
    /// Index into `candidates` of the peak the estimator actually selected.
    pub selected: usize,
    /// Selected BPM (== [`estimate_tempo`]'s `bpm`). This is octave-resolved, so it may be a
    /// ½×/2× octave of `candidates[selected]` when the dance-tempo prior breaks a 2:1 tie.
    pub selected_bpm: f32,
    /// Selected beat phase: first beat time in seconds (== [`BeatGrid::first_beat_sec`]).
    pub first_beat_sec: f32,
    /// Calibrated tempo confidence (== [`TempoEstimate::confidence`]). Note
    /// [`BeatGrid::confidence`] is this value *further* scaled by phase sharpness, so it
    /// can be lower when the downbeat position is itself ambiguous.
    pub confidence: f32,
    /// Onset support at the half-tempo octave (double the period), peak-relative.
    /// `None` if that lag falls outside the envelope. May exceed `1.0` when the true
    /// tempo sits outside the search range — a clear half/double-ambiguity signal.
    pub half_tempo_score: Option<f32>,
    /// Onset support at the double-tempo octave (half the period), peak-relative.
    pub double_tempo_score: Option<f32>,
}

/// Tempo search range. Results outside are octave-folded into it.
const MIN_BPM: f32 = 70.0;
const MAX_BPM: f32 = 180.0;
const FRAME: usize = 1024;
const HOP: usize = 256;
/// Most diagnostic tempo candidates surfaced by [`estimate_tempo_diagnostics`].
const MAX_CANDIDATES: usize = 6;

/// Raw autocorrelation tempo analysis, shared by the estimator and the diagnostics
/// path so they can never disagree about which lag won.
struct TempoAnalysis {
    /// Mean-subtracted, half-wave-rectified onset envelope.
    env: Vec<f32>,
    /// Envelope sample rate (Hz): `sample_rate / HOP`.
    env_rate: f32,
    /// Envelope energy (Σ env²), used to normalize autocorrelation scores.
    energy: f32,
    /// Autocorrelation score for each lag in `lag_min..=lag_max`.
    r_by_lag: Vec<f32>,
    /// First lag covered by `r_by_lag`.
    lag_min: usize,
    /// Lag of the winning (largest-score) autocorrelation peak.
    best_lag: usize,
    /// Score of the winning peak (== fraction of envelope energy repeating at that lag,
    /// since the autocorrelation is normalized so `r(0) == 1`).
    best_r: f32,
}

/// Shared first stage: onset envelope → autocorrelation over the BPM search range.
fn analyze_tempo(samples: &[f32], sample_rate: u32) -> Option<TempoAnalysis> {
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
    let mut r_by_lag = Vec::with_capacity(lag_max - lag_min + 1);
    let mut best_lag = lag_min;
    let mut best_r = f32::MIN;
    for lag in lag_min..=lag_max {
        let mut acc = 0.0f32;
        for i in 0..(env.len() - lag) {
            acc += env[i] * env[i + lag];
        }
        let r = acc / energy;
        r_by_lag.push(r);
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
    }

    Some(TempoAnalysis {
        env,
        env_rate,
        energy,
        r_by_lag,
        lag_min,
        best_lag,
        best_r,
    })
}

impl TempoAnalysis {
    /// Strongest in-range autocorrelation peak that is neither the winner's own lobe nor
    /// one of its octave (½×, 2×) lobes, as a fraction of the winning peak (0..1). A high
    /// value means a genuinely *different* tempo competes with the selection. Octave lobes
    /// are excluded here so they are not double-counted — they are scored by
    /// [`Self::octave_support`].
    fn rival_score(&self) -> f32 {
        if self.best_r <= 0.0 {
            return 0.0;
        }
        const GUARD: f32 = 3.0;
        let best = self.best_lag as f32;
        let skip = [best, best * 0.5, best * 2.0];
        let mut rival = 0.0f32;
        for (i, &r) in self.r_by_lag.iter().enumerate() {
            let lag = (self.lag_min + i) as f32;
            if skip.iter().any(|&c| (lag - c).abs() <= GUARD) {
                continue;
            }
            rival = rival.max(r);
        }
        (rival / self.best_r).clamp(0.0, 1.0)
    }

    /// Peak-relative onset support at the half-tempo and double-tempo octaves, taking the
    /// larger of the two. `> 1` means a stronger octave sits outside the search range — a
    /// likely-wrong octave pick (the half/double trap).
    fn octave_support(&self) -> f32 {
        [
            (2.0 * self.best_lag as f32).round() as usize,
            (0.5 * self.best_lag as f32).round() as usize,
        ]
        .into_iter()
        .filter_map(|lag| autocorr_score(&self.env, lag, self.energy, self.best_r))
        .fold(0.0f32, f32::max)
    }

    /// Calibrated 0..1 tempo confidence.
    ///
    /// Built from three honest signals, multiplied together:
    /// * **periodic strength** — `best_r` is the fraction of onset-envelope energy that
    ///   repeats at the chosen period (the autocorrelation is normalized so `r(0) == 1`),
    ///   passed through a saturating map. This is what collapses confidence for noise,
    ///   silence, and weak/sparse onsets — peak *prominence* alone cannot, because the max
    ///   of many near-zero lags still towers over their mean.
    /// * **octave factor** — discounts half/double ambiguity; gentle near 2:1 parity
    ///   (a genuine click ambiguity), steep once an unseen octave dominates the winner.
    /// * **rival factor** — discounts a competing in-range tempo.
    ///
    /// The net effect: a clean click reads as trustworthy, an octave-ambiguous track less
    /// so, and noise/silence near zero — an honest input for "verify beatgrid" logic.
    fn confidence(&self) -> f32 {
        if self.best_r <= 0.0 {
            return 0.0;
        }
        // Saturating: best_r 0.15 -> 0.5, 0.45 -> 0.75, ~0.9 -> 0.86. Tracks with weakly
        // periodic envelopes still earn moderate confidence; noise (best_r ~1e-3) -> ~0.
        const STRENGTH_HALF: f32 = 0.15;
        let strength = self.best_r / (self.best_r + STRENGTH_HALF);
        let octave_factor = 1.0 / (1.0 + 0.6 * self.octave_support().powi(2));
        let rival_factor = (1.0 - 0.5 * self.rival_score()).clamp(0.0, 1.0);
        (strength * octave_factor * rival_factor).clamp(0.0, 1.0)
    }

    /// Octave-aware tempo pick, returning the selected `(folded_bpm, period_in_env_samples)`.
    ///
    /// The largest autocorrelation peak is not always the musically-correct octave: a track
    /// with strong half-note accents can peak at half the perceived tempo, and a busy
    /// eighth-note groove can peak at double. Instead of trusting that peak blindly, we score
    /// the winner against its ½× and 2× octaves by `onset_support × tempo_prior(bpm)` and take
    /// the best. `onset_support` keeps the pick honest (an octave with no onsets can't win);
    /// the prior (see [`tempo_prior`]) gently resolves genuine 2:1 ambiguity toward the
    /// beat-matchable range. Folding still confines the result to [`MIN_BPM`, `MAX_BPM`].
    fn select_tempo(&self) -> (f32, f32) {
        let mut best_score = f32::MIN;
        let mut chosen = (0.0f32, 0.0f32);
        // 2× lag = half tempo, 0.5× lag = double tempo. Include 1× so the winner competes.
        for &mult in &[1.0f32, 2.0, 0.5] {
            let lag = self.best_lag as f32 * mult;
            if lag < 1.0 || lag as usize >= self.env.len() {
                continue;
            }
            let support = autocorr_score(&self.env, lag.round() as usize, self.energy, self.best_r)
                .unwrap_or(0.0);
            let refined =
                parabolic_peak(&self.env, lag.round() as usize, self.energy).unwrap_or(lag);
            let mut bpm = 60.0 * self.env_rate / refined;
            let mut period = refined;
            while bpm < MIN_BPM {
                bpm *= 2.0;
                period *= 2.0;
            }
            while bpm > MAX_BPM {
                bpm /= 2.0;
                period /= 2.0;
            }
            let score = support * tempo_prior(bpm);
            if score > best_score {
                best_score = score;
                chosen = (bpm, period);
            }
        }
        chosen
    }
}

/// Perceptual dance-tempo preference, used only to disambiguate octave-related tempo
/// candidates (never to invent a tempo). A broad log-normal resonance peaking near a
/// comfortable beat-matching tempo: it nudges genuine 2:1 ties toward the danceable octave
/// while staying flat enough not to override a clearly-dominant tempo. Returns a weight in
/// `(0, 1]`. See `docs/research/summaries/half-double-tempo-scoring.md`.
fn tempo_prior(bpm: f32) -> f32 {
    /// Preferred tempo center (BPM).
    const PREF_BPM: f32 = 125.0;
    /// Width of the resonance in natural-log tempo units (wide → gentle preference).
    const SIGMA: f32 = 0.55;
    if bpm <= 0.0 {
        return 0.0;
    }
    let z = (bpm / PREF_BPM).ln() / SIGMA;
    (-0.5 * z * z).exp()
}

/// Octave-fold a BPM into [`MIN_BPM`, `MAX_BPM`].
fn fold_bpm(mut bpm: f32) -> f32 {
    if bpm <= 0.0 {
        return 0.0;
    }
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
    }
    bpm
}

/// Autocorrelation score at an arbitrary lag, normalized against the winning peak.
/// `None` when the lag falls outside the envelope (no octave neighbor to score).
fn autocorr_score(env: &[f32], lag: usize, energy: f32, best_r: f32) -> Option<f32> {
    if lag == 0 || lag >= env.len() || best_r <= 0.0 {
        return None;
    }
    let mut acc = 0.0f32;
    for i in 0..(env.len() - lag) {
        acc += env[i] * env[i + lag];
    }
    Some(((acc / energy) / best_r).clamp(0.0, 4.0))
}

/// Comb candidate phases [0, period) over the onset envelope and return the first-beat
/// time (seconds) with the most accumulated onset energy, plus a 0..1 phase confidence:
/// how sharply the winning phase stands above the mean phase energy. A flat comb (many
/// competing downbeat positions) yields low phase confidence.
fn comb_first_beat(env: &[f32], period: f32, sample_rate: u32) -> (f32, f32) {
    if period <= 0.0 {
        return (0.0, 0.0);
    }
    let mut best_phi = 0usize;
    let mut best_score = f32::MIN;
    let mut sum_score = 0.0f32;
    let phi_max = period.ceil().max(1.0) as usize;
    for phi in 0..phi_max {
        let mut score = 0.0f32;
        let mut idx = phi as f32;
        while (idx as usize) < env.len() {
            score += env[idx as usize];
            idx += period;
        }
        sum_score += score;
        if score > best_score {
            best_score = score;
            best_phi = phi;
        }
    }
    let mean_score = sum_score / phi_max as f32;
    let phase_confidence = if best_score > 0.0 {
        (1.0 - mean_score / best_score).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (
        best_phi as f32 * (HOP as f32 / sample_rate as f32),
        phase_confidence,
    )
}

/// Shared core: onset envelope → autocorrelation tempo. Returns the (mean-subtracted)
/// envelope, its sample rate, the period in env-samples, the BPM, and confidence.
fn tempo_core(samples: &[f32], sample_rate: u32) -> Option<(Vec<f32>, f32, f32, f32, f32)> {
    let a = analyze_tempo(samples, sample_rate)?;
    let (bpm, period) = a.select_tempo();
    let confidence = a.confidence();
    Some((a.env, a.env_rate, period, bpm, confidence))
}

/// Estimate tempo from mono PCM via autocorrelation of a spectral-flux onset envelope.
///
/// Pipeline: onset envelope (spectral flux) → mean-subtract → autocorrelation over the
/// lag range implied by [`MIN_BPM`, `MAX_BPM`] → pick the peak lag → parabolic
/// interpolation for sub-bin precision → convert lag to BPM.
///
/// This is intentionally a solid-but-simple estimator: no full beat tracking / dynamic
/// programming yet (that, plus downbeat/phase for the beatgrid, is the rest of P1). It
/// uses no GPL code (aubio is reference-only). Returns BPM + confidence only;
/// [`estimate_tempo_diagnostics`] exposes the candidate ranking behind that choice.
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

    // A beatgrid needs both tempo *and* phase right, so fold the phase sharpness into the
    // tempo confidence — an ambiguous downbeat position lowers the grid's trustworthiness.
    let (first_beat_sec, phase_confidence) = comb_first_beat(&env, period, sample_rate);
    BeatGrid {
        bpm,
        first_beat_sec,
        beat_interval_sec: if bpm > 0.0 { 60.0 / bpm } else { 0.0 },
        confidence: confidence * phase_confidence,
    }
}

/// Diagnostic-only tempo analysis: the ranked autocorrelation candidates, the selected
/// tempo and beat phase, and how much onset support the half/double octaves have.
///
/// This makes ambiguous beatgrid decisions (especially half/double traps) visible to
/// tooling and tests without changing what [`estimate_tempo`] / [`estimate_beatgrid`]
/// return. Offline only — same cost profile as [`estimate_tempo`]. Returns `None` for
/// signals too short/quiet to analyze.
pub fn estimate_tempo_diagnostics(samples: &[f32], sample_rate: u32) -> Option<TempoDiagnostics> {
    let a = analyze_tempo(samples, sample_rate)?;

    // Collect in-range local maxima of the autocorrelation as candidates, plus the
    // winning lag itself (so `selected` always resolves even if it sat on a plateau).
    let n = a.r_by_lag.len();
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for i in 0..n {
        let r = a.r_by_lag[i];
        let left = if i == 0 { f32::MIN } else { a.r_by_lag[i - 1] };
        let right = if i + 1 == n {
            f32::MIN
        } else {
            a.r_by_lag[i + 1]
        };
        if r > 0.0 && r >= left && r >= right {
            peaks.push((a.lag_min + i, r));
        }
    }
    if !peaks.iter().any(|&(lag, _)| lag == a.best_lag) {
        peaks.push((a.best_lag, a.best_r));
    }
    peaks.sort_by(|x, y| y.1.total_cmp(&x.1));
    peaks.truncate(MAX_CANDIDATES);

    let candidates: Vec<TempoCandidate> = peaks
        .iter()
        .map(|&(lag, r)| {
            let refined = parabolic_peak(&a.env, lag, a.energy).unwrap_or(lag as f32);
            let raw_bpm = 60.0 * a.env_rate / refined;
            TempoCandidate {
                lag: refined,
                raw_bpm,
                folded_bpm: fold_bpm(raw_bpm),
                score: if a.best_r > 0.0 {
                    (r / a.best_r).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            }
        })
        .collect();
    let selected = peaks
        .iter()
        .position(|&(lag, _)| lag == a.best_lag)
        .unwrap_or(0);

    // Selected tempo/phase via the same octave-aware pick as `tempo_core`, so the diagnostics
    // never disagree with the public API. `selected_bpm` may therefore be an octave of
    // `candidates[selected]` (the raw autocorrelation winner) when the prior resolves a 2:1 tie.
    let (selected_bpm, period) = a.select_tempo();

    // Octave neighbors of the *winning* lag: double the period == half tempo, and vice versa.
    let half_tempo_score = autocorr_score(
        &a.env,
        (2.0 * a.best_lag as f32).round() as usize,
        a.energy,
        a.best_r,
    );
    let double_tempo_score = autocorr_score(
        &a.env,
        (0.5 * a.best_lag as f32).round() as usize,
        a.energy,
        a.best_r,
    );

    Some(TempoDiagnostics {
        candidates,
        selected,
        selected_bpm,
        first_beat_sec: comb_first_beat(&a.env, period, sample_rate).0,
        confidence: a.confidence(),
        half_tempo_score,
        double_tempo_score,
    })
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
    fn diagnostics_match_public_estimator() {
        // The diagnostics path must never disagree with estimate_tempo / estimate_beatgrid:
        // it shares analyze_tempo and derives the selected tempo/phase identically.
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

        let diag = estimate_tempo_diagnostics(&samples, sr).expect("diagnostics");
        let tempo = estimate_tempo(&samples, sr);
        let grid = estimate_beatgrid(&samples, sr);

        assert!((diag.selected_bpm - tempo.bpm).abs() < 1e-3, "bpm drift");
        assert!(
            (diag.confidence - tempo.confidence).abs() < 1e-3,
            "conf drift"
        );
        assert!(
            (diag.first_beat_sec - grid.first_beat_sec).abs() < 1e-3,
            "phase drift"
        );
        assert!(!diag.candidates.is_empty());
        assert_eq!(diag.candidates[diag.selected].score, 1.0);
        // A clean 120 BPM click has no real half/double competitor near the winning peak.
        assert!(diag.double_tempo_score.unwrap_or(0.0) < 1.0);
    }

    #[test]
    fn confidence_calibration_orders_clean_above_ambiguous_above_noise() {
        let sr = 44_100u32;
        let click = |bpm: f32| -> Vec<f32> {
            let n = sr as usize * 16;
            let mut s = vec![0.0f32; n];
            let period = (sr as f32 * 60.0 / bpm) as usize;
            let mut t = 0;
            while t < n {
                for k in 0..64 {
                    if t + k < n {
                        s[t + k] = (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
                    }
                }
                t += period;
            }
            s
        };
        let clean = estimate_tempo(&click(128.0), sr).confidence;

        let mut state = 0x1234_5678u32;
        let noise: Vec<f32> = (0..(sr as usize * 8))
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        let noise_conf = estimate_tempo(&noise, sr).confidence;

        // Unstructured noise has almost no repeating onset energy -> near-zero confidence,
        // even though its best autocorrelation lag still towers over the mean.
        assert!(clean > 0.3, "clean click should stay trustworthy: {clean}");
        assert!(
            noise_conf < 0.05,
            "noise should be untrustworthy: {noise_conf}"
        );
        assert!(clean > noise_conf);
    }

    #[test]
    fn diagnostics_none_on_empty() {
        assert!(estimate_tempo_diagnostics(&[], 44_100).is_none());
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
