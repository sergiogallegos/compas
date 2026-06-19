//! Real-time-safe DSP primitives.
//!
//! RT-SAFE CONTRACT (applies to every `process*` method in this module):
//!   * no heap allocation, no `Vec`/`Box`/`String` construction
//!   * no locks, no syscalls, no I/O, no logging
//!   * bounded, data-independent execution time
//!   * no panics on the hot path (no indexing that can go out of bounds, no `unwrap`)
//!
//! Coefficient *computation* (e.g. [`BiquadCoeffs::low_shelf`]) uses transcendental
//! functions and is intended to run on the control thread; the computed coeffs are
//! then handed to the audio thread. Recomputing coeffs inside the callback is allowed
//! (it does not allocate) but is discouraged at audio rate — smooth a target instead.

use std::f32::consts::PI;

/// Transposed-Direct-Form-II biquad coefficients (normalized so `a0 == 1`).
#[derive(Debug, Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoeffs {
    /// Identity (pass-through) filter.
    pub const IDENTITY: BiquadCoeffs = BiquadCoeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// RBJ low-shelf. `gain_db` boosts/cuts below `freq`. Control-thread use.
    pub fn low_shelf(freq: f32, sample_rate: f32, gain_db: f32) -> Self {
        Self::shelf(freq, sample_rate, gain_db, true)
    }

    /// RBJ high-shelf. Control-thread use.
    pub fn high_shelf(freq: f32, sample_rate: f32, gain_db: f32) -> Self {
        Self::shelf(freq, sample_rate, gain_db, false)
    }

    /// RBJ peaking EQ at `freq` with quality `q`. Control-thread use.
    pub fn peaking(freq: f32, sample_rate: f32, gain_db: f32, q: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * (freq / sample_rate);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q.max(1e-4));

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha / a;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ resonant low-pass — the DJ filter knob (LPF side). Control-thread use.
    pub fn low_pass(freq: f32, sample_rate: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q.max(1e-4));
        let b1 = 1.0 - cos;
        let b0 = b1 / 2.0;
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// RBJ resonant high-pass — the DJ filter knob (HPF side). Control-thread use.
    pub fn high_pass(freq: f32, sample_rate: f32, q: f32) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q.max(1e-4));
        let b1 = -(1.0 + cos);
        let b0 = (1.0 + cos) / 2.0;
        let b2 = b0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    fn shelf(freq: f32, sample_rate: f32, gain_db: f32, low: bool) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * (freq / sample_rate);
        let (sin, cos) = w0.sin_cos();
        // RBJ shelf alpha with slope S = 1: sin/2 * sqrt((A + 1/A)(1/S - 1) + 2)
        // collapses to sin/2 * sqrt(2) when S = 1 (independent of A).
        let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let ap1 = a + 1.0;
        let am1 = a - 1.0;

        let (b0, b1, b2, a0, a1, a2) = if low {
            (
                a * (ap1 - am1 * cos + two_sqrt_a_alpha),
                2.0 * a * (am1 - ap1 * cos),
                a * (ap1 - am1 * cos - two_sqrt_a_alpha),
                ap1 + am1 * cos + two_sqrt_a_alpha,
                -2.0 * (am1 + ap1 * cos),
                ap1 + am1 * cos - two_sqrt_a_alpha,
            )
        } else {
            (
                a * (ap1 + am1 * cos + two_sqrt_a_alpha),
                -2.0 * a * (am1 + ap1 * cos),
                a * (ap1 + am1 * cos - two_sqrt_a_alpha),
                ap1 - am1 * cos + two_sqrt_a_alpha,
                2.0 * (am1 - ap1 * cos),
                ap1 - am1 * cos - two_sqrt_a_alpha,
            )
        };
        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    #[allow(clippy::too_many_arguments)]
    fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = 1.0 / a0;
        BiquadCoeffs {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
        }
    }
}

/// A single biquad filter section with per-channel state.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    coeffs: BiquadCoeffs,
    // Transposed Direct Form II state (one pair per mono stream).
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coeffs: BiquadCoeffs) -> Self {
        Biquad {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Swap coefficients without resetting state (click-free if change is small).
    /// RT-SAFE.
    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.coeffs = coeffs;
    }

    /// Process one sample. RT-SAFE.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = &self.coeffs;
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Per-channel 3-band EQ (low shelf / mid peak / high shelf), the standard DJ-mixer
/// layout. One [`ThreeBandEq`] instance per audio channel.
#[derive(Debug, Clone, Copy)]
pub struct ThreeBandEq {
    low: Biquad,
    mid: Biquad,
    high: Biquad,
}

impl ThreeBandEq {
    /// Flat EQ at the given sample rate.
    pub fn new(sample_rate: f32) -> Self {
        let _ = sample_rate;
        ThreeBandEq {
            low: Biquad::new(BiquadCoeffs::IDENTITY),
            mid: Biquad::new(BiquadCoeffs::IDENTITY),
            high: Biquad::new(BiquadCoeffs::IDENTITY),
        }
    }

    /// Recompute coefficients for the three gains (dB). Control-thread use.
    pub fn set_gains_db(&mut self, sample_rate: f32, low_db: f32, mid_db: f32, high_db: f32) {
        self.low
            .set_coeffs(BiquadCoeffs::low_shelf(200.0, sample_rate, low_db));
        self.mid
            .set_coeffs(BiquadCoeffs::peaking(1_000.0, sample_rate, mid_db, 0.9));
        self.high
            .set_coeffs(BiquadCoeffs::high_shelf(4_000.0, sample_rate, high_db));
    }

    /// Process one sample through all three bands. RT-SAFE.
    #[inline(always)]
    pub fn process(&mut self, x: f32) -> f32 {
        self.high.process(self.mid.process(self.low.process(x)))
    }
}

/// One-pole gain smoother to avoid zipper noise when a fader/knob jumps.
/// RT-SAFE.
#[derive(Debug, Clone, Copy)]
pub struct GainSmoother {
    current: f32,
    target: f32,
    coeff: f32,
}

impl GainSmoother {
    /// `time_ms` is the ~63% settle time of the one-pole.
    pub fn new(initial: f32, sample_rate: f32, time_ms: f32) -> Self {
        let tau = (time_ms / 1000.0).max(1e-4);
        let coeff = (-1.0 / (tau * sample_rate)).exp();
        GainSmoother {
            current: initial,
            target: initial,
            coeff,
        }
    }

    /// RT-SAFE.
    #[inline(always)]
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Advance one sample and return the smoothed gain. RT-SAFE.
    #[inline(always)]
    pub fn next_gain(&mut self) -> f32 {
        self.current = self.target + self.coeff * (self.current - self.target);
        self.current
    }
}

/// Equal-power crossfader. `position` in `[0, 1]`: 0 = full A, 1 = full B.
/// Equal-power (cosine/sine) law keeps perceived loudness constant through the blend,
/// which is what DJs expect — a linear fade dips ~3 dB in the middle.
#[derive(Debug, Clone, Copy)]
pub struct Crossfader {
    gain: GainSmoother,
}

impl Crossfader {
    pub fn new(sample_rate: f32) -> Self {
        Crossfader {
            gain: GainSmoother::new(0.5, sample_rate, 5.0),
        }
    }

    /// Set fader position `[0,1]`. RT-SAFE.
    #[inline(always)]
    pub fn set_position(&mut self, position: f32) {
        self.gain.set_target(position.clamp(0.0, 1.0));
    }

    /// Returns `(gain_a, gain_b)` for the next sample. RT-SAFE.
    #[inline(always)]
    pub fn next_gains(&mut self) -> (f32, f32) {
        let p = self.gain.next_gain();
        let angle = p * (PI / 2.0);
        let (sin, cos) = angle.sin_cos();
        (cos, sin)
    }
}

/// Stereo delay line with feedback — the core of an echo/delay FX.
///
/// The ring buffer is allocated **once at construction** (control/setup thread, e.g.
/// inside the mixer's `new`), never the audio callback. [`Delay::process`] is
/// allocation-free, branch-bounded, and RT-SAFE. The read position is fractional and the
/// delay length is one-pole smoothed, so changing the time glides like tape instead of
/// clicking — the classic echo "pitch bend" when you sweep the time.
pub struct Delay {
    left: Vec<f32>,
    right: Vec<f32>,
    capacity: usize,
    write: usize,
    sample_rate: f32,
    target_samples: f32,
    cur_samples: f32,
    glide_coeff: f32,
    feedback: f32,
    mix: f32,
}

impl Delay {
    /// Allocate a delay able to hold up to `max_seconds` of stereo audio. Allocation
    /// happens here (setup), so [`process`](Self::process) stays RT-safe.
    pub fn new(sample_rate: f32, max_seconds: f32) -> Self {
        let capacity = ((sample_rate * max_seconds).ceil() as usize).max(16);
        let glide_coeff = (-1.0 / (0.06 * sample_rate)).exp(); // ~60 ms tape glide
        let default = sample_rate * 0.375;
        Delay {
            left: vec![0.0; capacity],
            right: vec![0.0; capacity],
            capacity,
            write: 0,
            sample_rate,
            target_samples: default.min((capacity - 4) as f32),
            cur_samples: default.min((capacity - 4) as f32),
            glide_coeff,
            feedback: 0.4,
            mix: 0.0,
        }
    }

    /// Set the delay time in seconds (control thread). Clamped to the allocated range.
    /// RT-SAFE.
    #[inline]
    pub fn set_time_sec(&mut self, secs: f32) {
        let max = (self.capacity - 4) as f32;
        self.target_samples = (secs * self.sample_rate).clamp(4.0, max);
    }

    /// Feedback amount, 0..~0.95 (higher = more, longer-lasting repeats). RT-SAFE.
    #[inline]
    pub fn set_feedback(&mut self, fb: f32) {
        self.feedback = fb.clamp(0.0, 0.95);
    }

    /// Wet mix: 0 = dry (transparent), 1 = fully wet. RT-SAFE.
    #[inline]
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Zero the delay line — call when (re)engaging the effect so audio from a previous
    /// on-period doesn't burst back. A bounded memset; intended for occasional control
    /// events, not per-sample use.
    pub fn clear(&mut self) {
        self.left.iter_mut().for_each(|s| *s = 0.0);
        self.right.iter_mut().for_each(|s| *s = 0.0);
        self.cur_samples = self.target_samples;
        self.write = 0;
    }

    /// Process one stereo frame, returning the wet/dry mix. RT-SAFE.
    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // Glide the read distance toward the target for a click-free time change.
        self.cur_samples =
            self.target_samples + self.glide_coeff * (self.cur_samples - self.target_samples);
        let d = self.cur_samples;

        // Fractional read position `d` samples behind the write head.
        let mut read = self.write as f32 - d;
        if read < 0.0 {
            read += self.capacity as f32;
        }
        let i0 = (read.floor() as usize) % self.capacity;
        let frac = read - read.floor();
        let i1 = if i0 + 1 == self.capacity { 0 } else { i0 + 1 };

        let wet_l = self.left[i0] + (self.left[i1] - self.left[i0]) * frac;
        let wet_r = self.right[i0] + (self.right[i1] - self.right[i0]) * frac;

        // Feed input + feedback back into the line at the write head.
        self.left[self.write] = in_l + wet_l * self.feedback;
        self.right[self.write] = in_r + wet_r * self.feedback;
        self.write = if self.write + 1 == self.capacity { 0 } else { self.write + 1 };

        let dry = 1.0 - self.mix;
        (in_l * dry + wet_l * self.mix, in_r * dry + wet_r * self.mix)
    }
}

// ---------------------------------------------------------------------------------
// Reverb (Schroeder/Moorer-style: parallel damped combs → series allpass diffusers)
// ---------------------------------------------------------------------------------

/// A feedback comb with a one-pole low-pass in the loop (the "damp"). RT-SAFE process.
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32, // damping low-pass state
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(size: usize) -> Self {
        Comb {
            buf: vec![0.0; size.max(1)],
            idx: 0,
            store: 0.0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buf[self.idx];
        self.store = output * self.damp2 + self.store * self.damp1;
        self.buf[self.idx] = input + self.store * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        output
    }

    fn set_damp(&mut self, d: f32) {
        self.damp1 = d;
        self.damp2 = 1.0 - d;
    }

    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.store = 0.0;
    }
}

/// A Schroeder allpass diffuser. RT-SAFE process.
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
}

impl Allpass {
    fn new(size: usize) -> Self {
        Allpass {
            buf: vec![0.0; size.max(1)],
            idx: 0,
            feedback: 0.5,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.idx];
        let output = -input + bufout;
        self.buf[self.idx] = input + bufout * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        output
    }

    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
    }
}

const NUM_COMBS: usize = 8;
const NUM_ALLPASS: usize = 4;
/// Comb/allpass tunings, in samples at 44.1 kHz (scaled to the device rate). These are the
/// well-known prime-spaced delays used by the classic public-domain comb+allpass reverb.
const COMB_TUNING: [usize; NUM_COMBS] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; NUM_ALLPASS] = [556, 441, 341, 225];
/// Right-channel delay offset for stereo decorrelation.
const STEREO_SPREAD: usize = 23;
/// Fixed input gain — the combs amplify, so the input is heavily padded.
const FIXED_GAIN: f32 = 0.015;
/// Wet make-up so a moderate `mix` is audible.
const WET_SCALE: f32 = 3.0;

/// Schroeder/Moorer-style stereo reverb: 8 parallel damped comb filters per channel feeding 4
/// series allpass diffusers. All buffers are **pre-allocated at construction** (sized for
/// the device rate); [`Reverb::process`] is allocation-free and RT-SAFE. Driven from the
/// summed (mono) input — the stereo image comes from the per-channel delay spread.
pub struct Reverb {
    combs_l: [Comb; NUM_COMBS],
    combs_r: [Comb; NUM_COMBS],
    allpass_l: [Allpass; NUM_ALLPASS],
    allpass_r: [Allpass; NUM_ALLPASS],
    mix: f32,
}

impl Reverb {
    /// Allocate a reverb tuned for `sample_rate`. Allocation happens here (setup), so
    /// [`process`](Self::process) stays RT-safe.
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let sized = |tuning: usize, spread: usize| (((tuning + spread) as f32 * scale).round() as usize).max(1);
        let mut rv = Reverb {
            combs_l: std::array::from_fn(|i| Comb::new(sized(COMB_TUNING[i], 0))),
            combs_r: std::array::from_fn(|i| Comb::new(sized(COMB_TUNING[i], STEREO_SPREAD))),
            allpass_l: std::array::from_fn(|i| Allpass::new(sized(ALLPASS_TUNING[i], 0))),
            allpass_r: std::array::from_fn(|i| Allpass::new(sized(ALLPASS_TUNING[i], STEREO_SPREAD))),
            mix: 0.0,
        };
        rv.set_room_size(0.5);
        rv.set_damp(0.5);
        rv
    }

    /// Room size 0..1 — larger = longer tail (higher comb feedback). RT-SAFE.
    pub fn set_room_size(&mut self, size: f32) {
        // roomsize feedback = size * scaleroom + offsetroom.
        let fb = size.clamp(0.0, 1.0) * 0.28 + 0.7;
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.feedback = fb;
        }
    }

    /// High-frequency damping 0..1 (more = darker tail). RT-SAFE.
    pub fn set_damp(&mut self, damp: f32) {
        let d = damp.clamp(0.0, 1.0) * 0.4; // damp scale
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.set_damp(d);
        }
    }

    /// Wet mix: 0 = dry, 1 = fully wet. RT-SAFE.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Zero all comb/allpass buffers — call when (re)engaging so an old tail doesn't
    /// reappear. Bounded; intended for occasional control events, not per-sample use.
    pub fn clear(&mut self) {
        self.combs_l.iter_mut().chain(self.combs_r.iter_mut()).for_each(Comb::clear);
        self.allpass_l.iter_mut().chain(self.allpass_r.iter_mut()).for_each(Allpass::clear);
    }

    /// Process one stereo frame, returning the wet/dry mix. RT-SAFE.
    #[inline]
    pub fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let input = (in_l + in_r) * FIXED_GAIN;
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for i in 0..NUM_COMBS {
            out_l += self.combs_l[i].process(input);
            out_r += self.combs_r[i].process(input);
        }
        for i in 0..NUM_ALLPASS {
            out_l = self.allpass_l[i].process(out_l);
            out_r = self.allpass_r[i].process(out_r);
        }
        let dry = 1.0 - self.mix;
        let wet = self.mix * WET_SCALE;
        (in_l * dry + out_l * wet, in_r * dry + out_r * wet)
    }
}

// ---------------------------------------------------------------------------------
// Time-stretch (WSOLA) — key-lock / master-tempo
// ---------------------------------------------------------------------------------

/// Grain (window) length in output samples. 2048 ≈ 43 ms @ 48 kHz — long enough for
/// solid bass, the cost being ~43 ms of key-lock latency.
const STRETCH_WINDOW: usize = 2048;
/// Synthesis hop. 50% overlap so a periodic Hann window sums to unity (no normalization).
const STRETCH_HOP: usize = STRETCH_WINDOW / 2;
/// Similarity-search radius (± output samples) for phase-continuous grain placement.
const STRETCH_SEARCH: usize = 384;

/// WSOLA time-stretcher for **key-lock** (change tempo without changing pitch).
///
/// Reads grains directly from the fully-decoded source buffer, so it needs no input
/// ring. The caller advances the play-head as usual (`base_ratio * tempo` per output
/// frame) and passes it as the per-sample `anchor`; the stretcher overlap-adds Hann
/// grains stepped at `base_ratio` (→ pitch preserved) and uses waveform-similarity search
/// to keep successive grains phase-continuous. All buffers are pre-allocated; per-sample
/// processing is allocation-free and RT-SAFE (a similarity search runs once per hop).
pub struct TimeStretch {
    win: Vec<f32>,
    accum_l: Vec<f32>,
    accum_r: Vec<f32>,
    cand_mono: Vec<f32>,
    template: Vec<f32>,
    emit_idx: usize,
    prev_pos: f64,
    primed: bool,
}

impl TimeStretch {
    pub fn new() -> Self {
        let n = STRETCH_WINDOW;
        let win = (0..n)
            .map(|k| 0.5 * (1.0 - (2.0 * PI * k as f32 / n as f32).cos()))
            .collect();
        TimeStretch {
            win,
            accum_l: vec![0.0; n],
            accum_r: vec![0.0; n],
            cand_mono: vec![0.0; 2 * STRETCH_SEARCH + STRETCH_HOP],
            template: vec![0.0; STRETCH_HOP],
            emit_idx: 0,
            prev_pos: 0.0,
            primed: false,
        }
    }

    /// Drop buffered output and re-prime on the next sample (call on engage / seek /
    /// after scratching, where the play-head jumps discontinuously). RT-SAFE.
    pub fn reset(&mut self) {
        self.primed = false;
        self.emit_idx = 0;
    }

    /// Pull one device-rate stereo frame. `samples` is interleaved stereo source of
    /// `frames` frames; `base_ratio` = source_rate/device_rate; `anchor` is the current
    /// play-head (source frames). RT-SAFE. Pitch is preserved; tempo is whatever rate the
    /// caller advances `anchor` at.
    #[inline]
    pub fn next_frame(&mut self, samples: &[f32], frames: usize, base_ratio: f64, anchor: f64) -> (f32, f32) {
        if !self.primed {
            self.prime(samples, frames, base_ratio, anchor);
        }
        let l = self.accum_l[self.emit_idx];
        let r = self.accum_r[self.emit_idx];
        self.emit_idx += 1;
        if self.emit_idx >= STRETCH_HOP {
            self.advance(samples, frames, base_ratio, anchor);
            self.emit_idx = 0;
        }
        (l, r)
    }

    fn prime(&mut self, samples: &[f32], frames: usize, base_ratio: f64, anchor: f64) {
        self.accum_l.iter_mut().for_each(|x| *x = 0.0);
        self.accum_r.iter_mut().for_each(|x| *x = 0.0);
        self.overlap_add(samples, frames, base_ratio, anchor);
        self.prev_pos = anchor;
        self.emit_idx = 0;
        self.primed = true;
    }

    fn advance(&mut self, samples: &[f32], frames: usize, base_ratio: f64, anchor: f64) {
        // Slide the finished hop out; the second half becomes the new overlap region.
        self.accum_l.copy_within(STRETCH_HOP.., 0);
        self.accum_r.copy_within(STRETCH_HOP.., 0);
        for i in (STRETCH_WINDOW - STRETCH_HOP)..STRETCH_WINDOW {
            self.accum_l[i] = 0.0;
            self.accum_r[i] = 0.0;
        }
        let pos = self.best_grain(samples, frames, base_ratio, anchor);
        self.overlap_add(samples, frames, base_ratio, pos);
        self.prev_pos = pos;
    }

    /// Similarity search: pick the grain near `anchor` whose head best matches the
    /// previous grain's natural continuation (its overlap tail), for phase continuity.
    fn best_grain(&mut self, samples: &[f32], frames: usize, base_ratio: f64, anchor: f64) -> f64 {
        for j in 0..STRETCH_HOP {
            let pos = self.prev_pos + ((STRETCH_HOP + j) as f64) * base_ratio;
            self.template[j] = lerp_mono(samples, frames, pos);
        }
        let span = 2 * STRETCH_SEARCH + STRETCH_HOP;
        for i in 0..span {
            let pos = anchor + ((i as f64) - STRETCH_SEARCH as f64) * base_ratio;
            self.cand_mono[i] = lerp_mono(samples, frames, pos);
        }
        let mut best_o = STRETCH_SEARCH; // offset 0 (= anchor) by default
        let mut best = f32::NEG_INFINITY;
        for o in 0..=(2 * STRETCH_SEARCH) {
            let mut acc = 0.0f32;
            for j in 0..STRETCH_HOP {
                acc += self.template[j] * self.cand_mono[o + j];
            }
            if acc > best {
                best = acc;
                best_o = o;
            }
        }
        anchor + ((best_o as f64) - STRETCH_SEARCH as f64) * base_ratio
    }

    fn overlap_add(&mut self, samples: &[f32], frames: usize, base_ratio: f64, pos: f64) {
        for k in 0..STRETCH_WINDOW {
            let w = self.win[k];
            let sp = pos + (k as f64) * base_ratio;
            self.accum_l[k] += w * lerp_ch(samples, frames, sp, 0);
            self.accum_r[k] += w * lerp_ch(samples, frames, sp, 1);
        }
    }
}

impl Default for TimeStretch {
    fn default() -> Self {
        Self::new()
    }
}

/// Linear interpolation of one channel of an interleaved-stereo buffer at a fractional
/// frame position (clamped to the buffer). RT-SAFE.
#[inline]
fn lerp_ch(samples: &[f32], frames: usize, pos: f64, ch: usize) -> f32 {
    if frames == 0 {
        return 0.0;
    }
    let p = pos.clamp(0.0, (frames - 1) as f64);
    let i = p.floor() as usize;
    let f = (p - i as f64) as f32;
    let a = samples[i * 2 + ch];
    let b = samples[(i + 1).min(frames - 1) * 2 + ch];
    a + (b - a) * f
}

#[inline]
fn lerp_mono(samples: &[f32], frames: usize, pos: f64) -> f32 {
    0.5 * (lerp_ch(samples, frames, pos, 0) + lerp_ch(samples, frames, pos, 1))
}

// ---------------------------------------------------------------------------------
// Synth (polyphonic instrument — playable from the on-screen keyboard or MIDI)
// ---------------------------------------------------------------------------------

/// Number of simultaneous synth voices. Note-on past this steals the quietest voice.
const SYNTH_VOICES: usize = 16;

/// Oscillator shape for the synth instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

impl Waveform {
    /// Map a small integer (from the UI/IPC) to a waveform.
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => Waveform::Sine,
            1 => Waveform::Triangle,
            2 => Waveform::Saw,
            _ => Waveform::Square,
        }
    }

    #[inline]
    fn sample(self, phase: f32) -> f32 {
        match self {
            Waveform::Sine => (2.0 * PI * phase).sin(),
            Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    note: u8,
    phase: f32,
    phase_inc: f32,
    vel: f32,
    env: f32,
    stage: EnvStage,
}

impl Voice {
    const fn idle() -> Self {
        Voice {
            note: 0,
            phase: 0.0,
            phase_inc: 0.0,
            vel: 0.0,
            env: 0.0,
            stage: EnvStage::Off,
        }
    }
}

/// A small polyphonic subtractive-ish synth: per-voice oscillator + linear ADSR envelope.
/// All state is fixed-size, so [`Synth::process`] is allocation-free and RT-SAFE. Driven by
/// `note_on`/`note_off` (from the on-screen keyboard or a MIDI controller).
pub struct Synth {
    voices: [Voice; SYNTH_VOICES],
    sample_rate: f32,
    waveform: Waveform,
    attack_rate: f32,
    decay_rate: f32,
    sustain: f32,
    release_rate: f32,
    gain: f32,
}

impl Synth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let per = |secs: f32| 1.0 / (secs * sr).max(1.0); // env units per sample
        Synth {
            voices: [Voice::idle(); SYNTH_VOICES],
            sample_rate: sr,
            waveform: Waveform::Triangle,
            attack_rate: per(0.004),
            decay_rate: per(0.25),
            sustain: 0.4,
            release_rate: per(0.3),
            gain: 0.6,
        }
    }

    pub fn set_waveform(&mut self, w: Waveform) {
        self.waveform = w;
    }

    /// Output level of the instrument, 0..~1. RT-SAFE.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 1.5);
    }

    /// Trigger a note (MIDI note number, velocity 0..127). RT-SAFE.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.note_off(note);
            return;
        }
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        // Reuse a voice already on this note, else a free one, else steal the quietest.
        let mut idx = None;
        let mut free = None;
        let (mut quiet_i, mut quiet_env) = (0usize, f32::INFINITY);
        for (i, v) in self.voices.iter().enumerate() {
            if v.stage != EnvStage::Off && v.note == note {
                idx = Some(i);
                break;
            }
            if v.stage == EnvStage::Off && free.is_none() {
                free = Some(i);
            }
            if v.env < quiet_env {
                quiet_env = v.env;
                quiet_i = i;
            }
        }
        let i = idx.or(free).unwrap_or(quiet_i);
        let v = &mut self.voices[i];
        v.note = note;
        v.phase_inc = freq / self.sample_rate;
        v.vel = velocity as f32 / 127.0;
        v.stage = EnvStage::Attack;
        // Keep phase/env on retrigger to avoid a click.
    }

    /// Release any voices playing `note`. RT-SAFE.
    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.stage != EnvStage::Off && v.stage != EnvStage::Release && v.note == note {
                v.stage = EnvStage::Release;
            }
        }
    }

    /// Release every voice (panic button / on disconnect). RT-SAFE.
    pub fn all_notes_off(&mut self) {
        for v in self.voices.iter_mut() {
            if v.stage != EnvStage::Off {
                v.stage = EnvStage::Release;
            }
        }
    }

    /// Process one sample (mono). RT-SAFE.
    #[inline]
    pub fn process(&mut self) -> f32 {
        let mut out = 0.0f32;
        for v in self.voices.iter_mut() {
            if v.stage == EnvStage::Off {
                continue;
            }
            // Advance the ADSR envelope.
            match v.stage {
                EnvStage::Attack => {
                    v.env += self.attack_rate;
                    if v.env >= 1.0 {
                        v.env = 1.0;
                        v.stage = EnvStage::Decay;
                    }
                }
                EnvStage::Decay => {
                    v.env -= self.decay_rate;
                    if v.env <= self.sustain {
                        v.env = self.sustain;
                        v.stage = EnvStage::Sustain;
                    }
                }
                EnvStage::Sustain => {}
                EnvStage::Release => {
                    v.env -= self.release_rate;
                    if v.env <= 0.0 {
                        v.env = 0.0;
                        v.stage = EnvStage::Off;
                    }
                }
                EnvStage::Off => {}
            }
            out += self.waveform.sample(v.phase) * v.env * v.vel;
            v.phase += v.phase_inc;
            if v.phase >= 1.0 {
                v.phase -= 1.0;
            }
        }
        out * self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_biquad_passes_signal() {
        let mut b = Biquad::new(BiquadCoeffs::IDENTITY);
        for x in [0.0, 1.0, -0.5, 0.25] {
            assert!((b.process(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn equal_power_crossfade_is_unity_power() {
        let mut xf = Crossfader::new(48_000.0);
        xf.set_position(0.5);
        // Let the smoother settle.
        let mut gains = (0.0, 0.0);
        for _ in 0..4096 {
            gains = xf.next_gains();
        }
        let power = gains.0 * gains.0 + gains.1 * gains.1;
        assert!((power - 1.0).abs() < 1e-3, "power was {power}");
    }

    #[test]
    fn gain_smoother_converges_to_target() {
        let mut g = GainSmoother::new(0.0, 48_000.0, 5.0);
        g.set_target(1.0);
        let mut v = 0.0;
        for _ in 0..48_000 {
            v = g.next_gain();
        }
        assert!((v - 1.0).abs() < 1e-3);
    }

    #[test]
    fn low_pass_attenuates_nyquist() {
        let coeffs = BiquadCoeffs::low_pass(1_000.0, 48_000.0, 0.707);
        let mut b = Biquad::new(coeffs);
        // Feed an alternating (near-Nyquist) signal; output should shrink.
        let mut last = 0.0;
        for i in 0..2048 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            last = b.process(x);
        }
        assert!(last.abs() < 0.5, "near-Nyquist not attenuated: {last}");
    }

    #[test]
    fn delay_is_transparent_when_dry() {
        let mut d = Delay::new(48_000.0, 1.0);
        d.set_mix(0.0);
        for x in [0.3, -0.7, 0.1, 0.9] {
            let (l, r) = d.process(x, x);
            assert!((l - x).abs() < 1e-6 && (r - x).abs() < 1e-6, "not transparent: {l}");
        }
    }

    #[test]
    fn delay_reproduces_impulse_after_delay_time() {
        let sr = 48_000.0;
        let mut d = Delay::new(sr, 1.0);
        d.set_time_sec(480.0 / sr); // exactly 480 samples
        d.set_feedback(0.0);
        d.set_mix(1.0);
        d.clear(); // snap the time-glide to target (and zero the line)
        d.process(1.0, 1.0); // single impulse
        let (mut peak, mut peak_idx) = (0.0f32, 0usize);
        for i in 0..1_000 {
            let l = d.process(0.0, 0.0).0.abs();
            if l > peak {
                peak = l;
                peak_idx = i;
            }
        }
        assert!(peak > 0.5, "echo not reproduced (peak {peak})");
        assert!((peak_idx as i32 - 479).abs() < 4, "echo at {peak_idx}, expected ~479");
    }

    #[test]
    fn delay_feedback_decays() {
        let sr = 48_000.0;
        let mut d = Delay::new(sr, 1.0);
        d.set_time_sec(240.0 / sr);
        d.set_feedback(0.5);
        d.set_mix(1.0);
        d.clear();
        d.process(1.0, 1.0);
        let out: Vec<f32> = (0..600).map(|_| d.process(0.0, 0.0).0.abs()).collect();
        let first = out[180..300].iter().cloned().fold(0.0f32, f32::max);
        let second = out[420..540].iter().cloned().fold(0.0f32, f32::max);
        assert!(first > 0.4, "first echo weak: {first}");
        assert!(
            second > 0.1 && second < first,
            "second echo {second} should be a decayed {first}"
        );
    }

    #[test]
    fn reverb_is_dry_at_zero_mix() {
        let mut r = Reverb::new(48_000.0);
        r.set_mix(0.0);
        for x in [0.4, -0.6, 0.2] {
            let (l, rr) = r.process(x, x);
            assert!((l - x).abs() < 1e-6 && (rr - x).abs() < 1e-6, "not dry: {l}");
        }
    }

    #[test]
    fn reverb_tail_rings_and_decays() {
        let sr = 48_000.0;
        let mut r = Reverb::new(sr);
        r.set_room_size(0.8);
        r.set_mix(1.0);
        r.clear();
        r.process(1.0, 1.0); // impulse
        let (mut early, mut late) = (0.0f32, 0.0f32);
        for i in 0..48_000 {
            let l = r.process(0.0, 0.0).0;
            if i < 4_000 {
                early += l * l;
            } else if (20_000..24_000).contains(&i) {
                late += l * l;
            }
        }
        assert!(early > 1e-6, "no reverb tail (early energy {early})");
        assert!(late < early, "reverb did not decay (early {early}, late {late})");
    }

    #[test]
    fn synth_sounds_then_releases_to_silence() {
        let mut s = Synth::new(48_000.0);
        s.note_on(69, 100); // A4
        let mut peak = 0.0f32;
        for _ in 0..4_800 {
            peak = peak.max(s.process().abs());
        }
        assert!(peak > 0.05, "note produced no sound (peak {peak})");
        s.note_off(69);
        for _ in 0..48_000 {
            s.process(); // let the release finish (300 ms ≪ 1 s)
        }
        let mut after = 0.0f32;
        for _ in 0..1_000 {
            after = after.max(s.process().abs());
        }
        assert!(after < 1e-4, "note did not release to silence (residual {after})");
    }

    #[test]
    fn synth_pitch_tracks_note() {
        // A sine voice at A4 (440 Hz) should cross zero ~880 times/sec.
        let mut s = Synth::new(48_000.0);
        s.set_waveform(Waveform::Sine);
        s.note_on(69, 127);
        let out: Vec<f32> = (0..48_000).map(|_| s.process()).collect();
        let win = &out[4_800..43_200]; // skip attack, steady sustain
        let zc = win.windows(2).filter(|w| w[0].signum() != w[1].signum() && w[0] != 0.0).count();
        let expected = (win.len() as f64 / 48_000.0 * 880.0) as i64;
        assert!(
            (zc as i64 - expected).abs() <= expected / 20,
            "zero-crossings {zc}, expected ~{expected}"
        );
    }

    #[test]
    fn synth_is_polyphonic() {
        let mut s = Synth::new(48_000.0);
        s.set_waveform(Waveform::Sine);
        s.note_on(60, 100);
        let one: f32 = (0..2_400).map(|_| s.process().abs()).fold(0.0, f32::max);
        let mut s2 = Synth::new(48_000.0);
        s2.set_waveform(Waveform::Sine);
        s2.note_on(60, 100);
        s2.note_on(67, 100); // a fifth on top
        let two: f32 = (0..2_400).map(|_| s2.process().abs()).fold(0.0, f32::max);
        assert!(two > one * 1.2, "second note did not add (one={one}, two={two})");
    }

    /// Build an interleaved-stereo sine of the given period (frames), `n` frames long.
    fn sine_src(period: f64, n: usize) -> Vec<f32> {
        let mut s = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = (2.0 * std::f64::consts::PI * i as f64 / period).sin() as f32;
            s.push(v);
            s.push(v);
        }
        s
    }

    /// Count sign changes — a proxy for frequency that's independent of duration.
    fn zero_crossings(xs: &[f32]) -> usize {
        xs.windows(2).filter(|w| w[0].signum() != w[1].signum() && w[0] != 0.0).count()
    }

    #[test]
    fn time_stretch_preserves_pitch_when_slowed() {
        let period = 100.0; // 480 Hz @ 48 kHz
        let frames = 48_000;
        let src = sine_src(period, frames);
        let mut st = TimeStretch::new();
        // Slow to half speed: advance the anchor at 0.5 frames/output-sample.
        let tempo = 0.5;
        let mut anchor = 0.0f64;
        let mut out = Vec::with_capacity(8_000);
        for _ in 0..8_000 {
            out.push(st.next_frame(&src, frames, 1.0, anchor).0);
            anchor += tempo;
        }
        // Skip the prime/latency transient, measure a steady stretch of output.
        let win = &out[2_000..6_000];
        let zc = zero_crossings(win);
        // Pitch preserved ⇒ ~same period as the source: 4000/100*2 = 80 crossings.
        // Varispeed at 0.5 would instead halve it to ~40.
        let expected = (win.len() as f64 / period * 2.0) as i64;
        assert!(
            (zc as i64 - expected).abs() <= expected / 5,
            "zero-crossings {zc}, expected ~{expected} (pitch not preserved?)"
        );
    }

    #[test]
    fn time_stretch_preserves_pitch_when_sped_up() {
        let period = 160.0; // 300 Hz @ 48 kHz
        let frames = 48_000;
        let src = sine_src(period, frames);
        let mut st = TimeStretch::new();
        let tempo = 1.5; // faster
        let mut anchor = 0.0f64;
        let mut out = Vec::with_capacity(8_000);
        for _ in 0..8_000 {
            out.push(st.next_frame(&src, frames, 1.0, anchor).0);
            anchor += tempo;
        }
        let win = &out[2_000..6_000];
        let zc = zero_crossings(win);
        let expected = (win.len() as f64 / period * 2.0) as i64;
        assert!(
            (zc as i64 - expected).abs() <= expected / 5,
            "zero-crossings {zc}, expected ~{expected} (pitch not preserved?)"
        );
    }

    #[test]
    fn time_stretch_output_is_bounded() {
        let src = sine_src(73.0, 20_000);
        let mut st = TimeStretch::new();
        let mut anchor = 0.0f64;
        let mut peak = 0.0f32;
        for _ in 0..10_000 {
            let (l, r) = st.next_frame(&src, 20_000, 1.0, anchor);
            assert!(l.is_finite() && r.is_finite(), "non-finite output");
            peak = peak.max(l.abs());
            anchor += 1.07;
        }
        assert!(peak > 0.3, "output suspiciously quiet (peak {peak})");
        assert!(peak < 1.6, "output overshoots badly (peak {peak})");
    }
}
