//! Live / online beat tracking — adoption-plan slice 5.
//! See `docs/research/live-input-beat-tracking.md` for the full design.
//!
//! A **causal** tempo + beat-phase tracker for live input (mic/aux/external gear): it only ever
//! sees the past, induces tempo over a sliding window of the onset envelope, and tracks beat phase
//! by combing that window each update while a forward oscillator advances the phase between updates.
//!
//! This is **not** for offline local files (use [`crate::analysis::estimate_beatgrid`], which can
//! use future context) and **not** for the audio callback: it allocates in [`LiveTracker::new`] and
//! is meant to run on a separate analysis thread. After `new` it is allocation-free, and per-chunk
//! cost is bounded and independent of total stream length.
//!
//! [`LiveTracker::push`] accepts arbitrary-sized mono chunks; chunk boundaries never affect the
//! output (partial hops are buffered), so a signal fed in one call yields the same result as the
//! same signal fed in many — the no-look-ahead invariant.

use std::sync::Arc;

use rustfft::{num_complex::Complex32, Fft, FftPlanner};

use crate::analysis::{tempo_prior, FRAME, HOP, MAX_BPM, MIN_BPM};

/// A live tempo/phase estimate at the latest processed sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveEstimate {
    /// Current tempo estimate (BPM), octave-resolved into [`MIN_BPM`, `MAX_BPM`].
    pub bpm: f32,
    /// Beat phase in `[0, 1)`: fraction of the current beat elapsed at the latest processed hop
    /// (0 == on the beat). Only meaningful when `locked`.
    pub beat_phase: f32,
    /// 0..1 confidence — the fraction of windowed onset energy that repeats at the chosen period.
    pub confidence: f32,
    /// True once confidence has held above threshold for [`LOCK_UPDATES`] consecutive updates.
    pub locked: bool,
}

/// Sliding analysis window (seconds of onset envelope retained for tempo induction).
const WINDOW_SEC: f32 = 8.0;
/// Re-estimate tempo (and re-comb phase) every this many hops — bounds cost and is the PLL rate.
const TEMPO_UPDATE_HOPS: usize = 16;
/// Don't estimate until the window is at least this full (a couple of seconds of evidence).
const MIN_FILL_FRAC: f32 = 0.35;
/// Tempo smoothing: new estimate weight per update (1.0 on the first lock for a fast cold start).
const BPM_SMOOTH: f32 = 0.25;
/// Confidence to count toward lock, and how many consecutive updates to declare `locked`.
const LOCK_CONFIDENCE: f32 = 0.18;
const LOCK_UPDATES: u32 = 3;
/// Maps periodic strength (repeating onset-energy fraction) to a saturating 0..1 confidence.
const STRENGTH_HALF: f32 = 0.15;

/// Causal online beat tracker. Construct with [`LiveTracker::new`], feed mono samples with
/// [`LiveTracker::push`].
pub struct LiveTracker {
    env_rate: f32,
    // --- incremental spectral flux ---
    fft: Arc<dyn Fft<f32>>,
    hann: Vec<f32>,
    scratch: Vec<Complex32>,
    fft_buf: Vec<Complex32>,
    /// Circular buffer of the last `FRAME` input samples.
    sample_ring: Vec<f32>,
    write_idx: usize,
    filled: usize,
    since_hop: usize,
    prev_mag: Vec<f32>,
    // --- onset-envelope sliding window (ring of the last `window_len` env samples) ---
    env: Vec<f32>,
    env_pos: usize,
    env_filled: usize,
    window_len: usize,
    /// Scratch for the ordered (oldest→newest) window copy used by tempo/phase estimation.
    ordered: Vec<f32>,
    // --- tempo / phase state ---
    hops_since_tempo: usize,
    period_env: f32, // beat period in env-samples; 0 == not yet estimated
    bpm: f32,
    confidence: f32,
    phase: f32, // 0..1 within the beat at the latest processed hop
    lock_count: u32,
    locked: bool,
    have_estimate: bool,
}

impl LiveTracker {
    /// Build a tracker for `sample_rate` Hz mono input. Allocates all working buffers up front.
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(1) as f32;
        let env_rate = sr / HOP as f32;
        let window_len = ((WINDOW_SEC * env_rate) as usize).max(64);
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FRAME);
        let scratch = vec![Complex32::new(0.0, 0.0); fft.get_inplace_scratch_len()];
        let hann: Vec<f32> = (0..FRAME)
            .map(|n| {
                let x = (std::f32::consts::PI * n as f32 / (FRAME as f32 - 1.0)).sin();
                x * x
            })
            .collect();
        LiveTracker {
            env_rate,
            fft,
            hann,
            scratch,
            fft_buf: vec![Complex32::new(0.0, 0.0); FRAME],
            sample_ring: vec![0.0; FRAME],
            write_idx: 0,
            filled: 0,
            since_hop: 0,
            prev_mag: vec![0.0; FRAME / 2 + 1],
            env: vec![0.0; window_len],
            env_pos: 0,
            env_filled: 0,
            window_len,
            ordered: vec![0.0; window_len],
            hops_since_tempo: 0,
            period_env: 0.0,
            bpm: 0.0,
            confidence: 0.0,
            phase: 0.0,
            lock_count: 0,
            locked: false,
            have_estimate: false,
        }
    }

    /// The current best estimate, or `None` before the first tempo lock-on.
    pub fn estimate(&self) -> Option<LiveEstimate> {
        self.have_estimate.then_some(LiveEstimate {
            bpm: self.bpm,
            beat_phase: self.phase,
            confidence: self.confidence,
            locked: self.locked,
        })
    }

    /// Feed a chunk of mono samples. Processes every complete hop the chunk completes (buffering
    /// any partial hop for the next call) and returns the latest estimate, or `None` until the
    /// first tempo estimate is available.
    pub fn push(&mut self, samples: &[f32]) -> Option<LiveEstimate> {
        for &s in samples {
            self.sample_ring[self.write_idx] = s;
            self.write_idx = (self.write_idx + 1) % FRAME;
            if self.filled < FRAME {
                self.filled += 1;
            }
            self.since_hop += 1;
            if self.filled >= FRAME && self.since_hop >= HOP {
                self.since_hop -= HOP;
                self.process_hop();
            }
        }
        self.estimate()
    }

    /// One hop: compute the spectral-flux onset value for the latest `FRAME` window, push it to the
    /// envelope ring, advance the phase oscillator, and periodically re-estimate tempo + phase.
    fn process_hop(&mut self) {
        // Ordered (oldest→newest) windowed frame → FFT. When the ring is full, the oldest sample
        // sits at `write_idx` (the next slot to be overwritten).
        for k in 0..FRAME {
            let s = self.sample_ring[(self.write_idx + k) % FRAME];
            self.fft_buf[k] = Complex32::new(s * self.hann[k], 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.fft_buf, &mut self.scratch);
        let mut flux = 0.0f32;
        for (k, prev) in self.prev_mag.iter_mut().enumerate() {
            let mag = self.fft_buf[k].norm();
            let d = mag - *prev;
            if d > 0.0 {
                flux += d;
            }
            *prev = mag;
        }
        // Push flux into the envelope ring.
        self.env[self.env_pos] = flux;
        self.env_pos = (self.env_pos + 1) % self.window_len;
        if self.env_filled < self.window_len {
            self.env_filled += 1;
        }
        // Advance the beat-phase oscillator by one env-sample's worth of a beat.
        if self.period_env > 0.0 {
            self.phase += 1.0 / self.period_env;
            if self.phase >= 1.0 {
                self.phase -= self.phase.floor();
            }
        }
        self.hops_since_tempo += 1;
        let min_fill = (self.window_len as f32 * MIN_FILL_FRAC) as usize;
        if self.hops_since_tempo >= TEMPO_UPDATE_HOPS && self.env_filled >= min_fill {
            self.hops_since_tempo = 0;
            self.update_tempo_and_phase();
        }
    }

    /// Copy the envelope ring into `ordered` (oldest→newest) and return the valid length.
    fn fill_ordered(&mut self) -> usize {
        let n = self.env_filled;
        // Oldest sample index in the ring: when full, that's `env_pos`; otherwise the ring filled
        // from 0, so the oldest is at 0.
        let start = if self.env_filled == self.window_len {
            self.env_pos
        } else {
            0
        };
        for i in 0..n {
            self.ordered[i] = self.env[(start + i) % self.window_len];
        }
        n
    }

    /// Windowed autocorrelation tempo (octave-resolved by the shared dance-tempo prior) plus a
    /// causal comb for the beat phase, run over only the past `window` of onset energy.
    fn update_tempo_and_phase(&mut self) {
        let n = self.fill_ordered();
        if n < 16 {
            return;
        }
        // Mean-subtract + half-wave rectify the window in place.
        let mean = self.ordered[..n].iter().sum::<f32>() / n as f32;
        for v in self.ordered[..n].iter_mut() {
            *v = (*v - mean).max(0.0);
        }
        let env = &self.ordered[..n];
        let energy: f32 = env.iter().map(|x| x * x).sum::<f32>().max(1e-9);

        let lag_min = (60.0 * self.env_rate / MAX_BPM).floor().max(1.0) as usize;
        let lag_max = ((60.0 * self.env_rate / MIN_BPM).ceil() as usize).min(n - 1);
        if lag_max <= lag_min {
            return;
        }
        // Best autocorrelation peak in range.
        let mut best_lag = lag_min;
        let mut best_r = f32::MIN;
        for lag in lag_min..=lag_max {
            let r = autocorr(env, lag, energy);
            if r > best_r {
                best_r = r;
                best_lag = lag;
            }
        }
        if best_r <= 0.0 {
            return;
        }
        // Octave-resolve: score the winner and its ½×/2× octaves by support × dance-tempo prior,
        // exactly as the offline `select_tempo` does, so live and offline agree on the octave.
        let mut chosen_period = best_lag as f32;
        let mut chosen_score = f32::MIN;
        for &mult in &[1.0f32, 2.0, 0.5] {
            let lag = (best_lag as f32 * mult).round() as usize;
            if lag < 1 || lag >= n {
                continue;
            }
            let support = autocorr(env, lag, energy) / best_r;
            let mut bpm = 60.0 * self.env_rate / lag as f32;
            while bpm < MIN_BPM {
                bpm *= 2.0;
            }
            while bpm > MAX_BPM {
                bpm /= 2.0;
            }
            let score = support * tempo_prior(bpm);
            if score > chosen_score {
                chosen_score = score;
                chosen_period = 60.0 * self.env_rate / bpm; // period for the folded bpm
            }
        }
        let mut bpm = 60.0 * self.env_rate / chosen_period;
        while bpm < MIN_BPM {
            bpm *= 2.0;
        }
        while bpm > MAX_BPM {
            bpm /= 2.0;
        }
        let period = 60.0 * self.env_rate / bpm;

        // Confidence: saturating map of the repeating-energy fraction at the winning lag.
        let conf = (best_r / (best_r + STRENGTH_HALF)).clamp(0.0, 1.0);

        // Smooth tempo (snap on first lock for a fast cold start).
        if self.have_estimate {
            self.bpm = (1.0 - BPM_SMOOTH) * self.bpm + BPM_SMOOTH * bpm;
        } else {
            self.bpm = bpm;
        }
        self.period_env = 60.0 * self.env_rate / self.bpm;
        self.confidence = conf;
        self.have_estimate = true;

        // Causal phase: comb the past window at the chosen period and set the oscillator so the
        // latest sample sits at the right phase relative to the best beat offset.
        let phi = comb_phase(env, period);
        let last = (n - 1) as f32;
        let ph = ((last - phi) / period).rem_euclid(1.0);
        self.phase = if ph.is_finite() { ph } else { 0.0 };

        // Lock hysteresis.
        if conf >= LOCK_CONFIDENCE {
            self.lock_count = (self.lock_count + 1).min(LOCK_UPDATES);
            if self.lock_count >= LOCK_UPDATES {
                self.locked = true;
            }
        } else {
            self.lock_count = 0;
            self.locked = false;
        }
    }
}

/// Normalized autocorrelation of `env` at `lag` (fraction of energy repeating at that lag).
fn autocorr(env: &[f32], lag: usize, energy: f32) -> f32 {
    if lag == 0 || lag >= env.len() {
        return 0.0;
    }
    let mut acc = 0.0f32;
    for i in 0..(env.len() - lag) {
        acc += env[i] * env[i + lag];
    }
    acc / energy
}

/// Comb candidate phases `[0, period)` over `env` and return the offset (env-samples) with the most
/// accumulated onset energy — the best beat alignment over the past window.
fn comb_phase(env: &[f32], period: f32) -> f32 {
    if period <= 0.0 {
        return 0.0;
    }
    let phi_max = period.ceil().max(1.0) as usize;
    let mut best_phi = 0usize;
    let mut best = f32::MIN;
    for phi in 0..phi_max {
        let mut score = 0.0f32;
        let mut idx = phi as f32;
        while (idx as usize) < env.len() {
            score += env[idx as usize];
            idx += period;
        }
        if score > best {
            best = score;
            best_phi = phi;
        }
    }
    best_phi as f32
}
