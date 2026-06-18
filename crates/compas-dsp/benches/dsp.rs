//! Criterion benchmarks for the DSP hot loops. Run: `cargo bench -p compas-dsp`.
//! These exercise the per-sample RT path (biquad/EQ/crossfade) and the offline tempo
//! estimator, so regressions in either show up as wall-clock changes.

use compas_dsp::analysis::estimate_tempo;
use compas_dsp::{Biquad, BiquadCoeffs, Crossfader, Delay, Reverb, ThreeBandEq};
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

criterion_group!(
    benches,
    bench_biquad,
    bench_three_band_eq,
    bench_crossfader,
    bench_delay,
    bench_reverb,
    bench_tempo
);
criterion_main!(benches);
