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
}
