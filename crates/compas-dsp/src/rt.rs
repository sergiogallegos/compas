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
}
