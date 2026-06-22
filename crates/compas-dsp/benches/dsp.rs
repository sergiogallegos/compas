//! Criterion benchmarks for the DSP hot loops. Run: `cargo bench -p compas-dsp`.
//! These exercise the per-sample RT path (biquad/EQ/crossfade) and the offline tempo
//! estimator, so regressions in either show up as wall-clock changes.

use compas_dsp::analysis::{estimate_beatgrid, estimate_tempo};
use compas_dsp::{Biquad, BiquadCoeffs, Crossfader, Delay, Reverb, ThreeBandEq, TimeStretch};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

const BLOCK: usize = 1024;

fn bench_biquad(c: &mut Criterion) {
    let mut b = Biquad::new(BiquadCoeffs::low_pass(1_000.0, 48_000.0, 0.707));
    c.bench_function("biquad_process_block", |bn| {
        bn.iter(|| {
            let mut acc = 0.0f32;
            for i in 0..BLOCK {
                acc += b.process(black_box(if i % 2 == 0 { 0.5 } else { -0.5 }));
            }
            acc
        })
    });
}

fn bench_three_band_eq(c: &mut Criterion) {
    let mut eq = ThreeBandEq::new(48_000.0);
    eq.set_gains_db(48_000.0, 4.0, -3.0, 2.0);
    c.bench_function("three_band_eq_block", |bn| {
        bn.iter(|| {
            let mut acc = 0.0f32;
            for i in 0..BLOCK {
                acc += eq.process(black_box(if i % 2 == 0 { 0.5 } else { -0.5 }));
            }
            acc
        })
    });
}

fn bench_crossfader(c: &mut Criterion) {
    let mut xf = Crossfader::new(48_000.0);
    xf.set_position(0.5);
    c.bench_function("crossfader_block", |bn| {
        bn.iter(|| {
            let mut acc = 0.0f32;
            for _ in 0..BLOCK {
                let (a, b) = xf.next_gains();
                acc += a + b;
            }
            acc
        })
    });
}

fn bench_delay(c: &mut Criterion) {
    let mut d = Delay::new(48_000.0, 2.0);
    d.set_time_sec(0.375);
    d.set_feedback(0.45);
    d.set_mix(0.4);
    c.bench_function("delay_process_block", |bn| {
        bn.iter(|| {
            let mut acc = 0.0f32;
            for i in 0..BLOCK {
                let (l, r) = d.process(black_box(if i % 2 == 0 { 0.5 } else { -0.5 }), 0.25);
                acc += l + r;
            }
            acc
        })
    });
}

fn bench_reverb(c: &mut Criterion) {
    let mut r = Reverb::new(48_000.0);
    r.set_room_size(0.7);
    r.set_mix(0.3);
    c.bench_function("reverb_process_block", |bn| {
        bn.iter(|| {
            let mut acc = 0.0f32;
            for i in 0..BLOCK {
                let (l, rr) = r.process(black_box(if i % 2 == 0 { 0.5 } else { -0.5 }), 0.25);
                acc += l + rr;
            }
            acc
        })
    });
}

fn bench_time_stretch(c: &mut Criterion) {
    // A 1 s stereo sine as the source the stretcher reads grains from.
    let frames = 48_000usize;
    let mut src = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let v = (2.0 * std::f32::consts::PI * i as f32 / 100.0).sin();
        src.push(v);
        src.push(v);
    }
    let mut st = TimeStretch::new();
    c.bench_function("time_stretch_block", |bn| {
        bn.iter(|| {
            let mut anchor = 0.0f64;
            let mut acc = 0.0f32;
            for _ in 0..BLOCK {
                let (l, _) = st.next_frame(black_box(&src), frames, 1.0, anchor);
                acc += l;
                anchor += 1.06; // +6% key-locked
                if anchor > (frames - 4) as f64 {
                    anchor = 0.0;
                }
            }
            acc
        })
    });
}

fn bench_tempo(c: &mut Criterion) {
    // 8 s of a 128 BPM click — representative analysis input.
    let sr = 44_100u32;
    let period = (sr as f32 * 60.0 / 128.0) as usize;
    let total = sr as usize * 8;
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
    c.bench_function("estimate_tempo_8s", |bn| {
        bn.iter(|| estimate_tempo(black_box(&samples), sr))
    });
}

fn bench_tempo_signal_matrix(c: &mut Criterion) {
    // Cost of the tempo estimator across the signal shapes the eval matrix covers. Onset
    // density and clip length both drive the autocorrelation cost, so track them together.
    let sr = 44_100u32;
    let secs = 12.0;
    let n = (sr as f32 * secs) as usize;

    let add_click = |s: &mut [f32], at: f32| {
        let start = (at * sr as f32) as usize;
        for k in 0..64 {
            if let Some(v) = s.get_mut(start + k) {
                *v += (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
    };
    let clicks = |bpm: f32, first: f32| {
        let mut s = vec![0.0f32; n];
        let mut t = first;
        while t < secs {
            add_click(&mut s, t);
            t += 60.0 / bpm;
        }
        s
    };

    // Half/double trap: strong 64 BPM with weaker 128 BPM off-beats — double the onset density.
    let mut trap = clicks(64.0, 0.0);
    {
        let mut t = 60.0 / 128.0;
        while t < secs {
            let start = (t * sr as f32) as usize;
            for k in 0..64 {
                if let Some(v) = trap.get_mut(start + k) {
                    *v += 0.35 * (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
                }
            }
            t += 60.0 / 128.0;
        }
    }

    // Noise: no periodicity, but the same envelope/autocorrelation work runs.
    let mut state = 0x9E37_79B9u32;
    let noise: Vec<f32> = (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / u32::MAX as f32 - 0.5
        })
        .collect();

    for (name, samples) in [
        ("tempo_clean_128", clicks(128.0, 0.0)),
        ("tempo_half_double_trap", trap),
        ("tempo_noise", noise),
    ] {
        c.bench_function(name, |bn| {
            bn.iter(|| estimate_tempo(black_box(&samples), sr))
        });
    }
}

fn bench_beatgrid(c: &mut Criterion) {
    // 12 s of a delayed 124 BPM click track exercises both tempo and phase estimation.
    let sr = 44_100u32;
    let period = sr as f32 * 60.0 / 124.0;
    let total = sr as usize * 12;
    let mut samples = vec![0.0f32; total];
    let mut t = (sr as f32 * 0.25) as usize;
    while t < total {
        for k in 0..64 {
            if t + k < total {
                samples[t + k] = (1.0 - k as f32 / 64.0) * if k % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        t = (t as f32 + period).round() as usize;
    }
    c.bench_function("estimate_beatgrid_12s", |bn| {
        bn.iter(|| estimate_beatgrid(black_box(&samples), sr))
    });
}

criterion_group!(
    benches,
    bench_biquad,
    bench_three_band_eq,
    bench_crossfader,
    bench_delay,
    bench_reverb,
    bench_time_stretch,
    bench_tempo,
    bench_tempo_signal_matrix,
    bench_beatgrid
);
criterion_main!(benches);
