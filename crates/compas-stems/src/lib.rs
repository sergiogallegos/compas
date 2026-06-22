//! `compas-stems` — **offline** htdemucs stem separation via ONNX Runtime (`ort`).
//!
//! This crate is deliberately kept out of the real-time engine crates: it quarantines the one
//! non-pure-Rust dependency (the native onnxruntime, via `ort`) and runs entirely on a worker
//! thread, so it allocates freely and has **no** real-time-safety obligations. It turns a decoded
//! stereo mix into four stem buffers (`drums`, `bass`, `other`, `vocals`) that drop straight into
//! the engine as `DeckBuffer`s (S2 wiring lands separately).
//!
//! ## Model
//! The single-file **htdemucs** ONNX graph (`StemSplitio/htdemucs-onnx`, ~301 MB) has a baked-in
//! fixed segment shape `mix:[1, 2, 343980]` (7.8 s @ 44.1 kHz stereo) → `stems:[1, 4, 2, 343980]`.
//! STFT **and** the demucs mean/std normalization live *inside* the graph, so the host side only has
//! to chunk, window, and overlap-add — there is no spectrogram or normalization math here.
//! Scheme (quarter-segment overlap + triangular window) mirrors demucs' own internal stitching.
//!
//! ## Status (S1)
//! Implemented: the 44.1 kHz separation core + overlap-add. **Follow-ups:** rubato resampling for
//! non-44.1 kHz sources (today `separate` errors on a rate mismatch), the checksum'd
//! optional-download of the model into the app-data dir, and the engine/IPC wiring.

use std::path::Path;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

/// Sample rate the model operates at; inputs must already be at this rate (S1).
pub const MODEL_SAMPLE_RATE: u32 = 44_100;
/// Samples per segment, baked into the exported graph (7.8 s @ 44.1 kHz).
pub const N_SAMPLES: usize = 343_980;
/// The model is stereo in / stereo out per stem.
pub const N_CHANNELS: usize = 2;
/// Quarter-segment overlap between consecutive chunks.
const OVERLAP: usize = N_SAMPLES / 4; // 85_995
/// Hop between chunk starts.
const STRIDE: usize = N_SAMPLES - OVERLAP; // 257_985
/// Number of stems the single-file htdemucs model emits, in output-row order.
pub const SOURCES: [&str; 4] = ["drums", "bass", "other", "vocals"];

/// Errors from loading a model or running separation.
#[derive(Debug, thiserror::Error)]
pub enum StemError {
    /// Anything surfaced by the ONNX Runtime (load, shape, inference). Held as a string because
    /// `ort::Error` is generic over a context type (`Error<SessionBuilder>`, `Error<()>`, …),
    /// which a single `#[from]` can't cover.
    #[error("onnx runtime: {0}")]
    Ort(String),
    /// Input sample rate is not yet supported (resampling is a follow-up).
    #[error("input is {got} Hz; S1 requires {expected} Hz (resampling not wired yet)")]
    UnsupportedRate { got: u32, expected: u32 },
    /// Input buffer was not interleaved stereo (even length).
    #[error("expected interleaved stereo (even sample count), got {0} samples")]
    NotStereo(usize),
    /// The model returned an output whose element count doesn't match `4·2·N_SAMPLES`.
    #[error("model output had {got} elements, expected {expected}")]
    BadOutput { got: usize, expected: usize },
}

/// Four separated stems as **interleaved stereo `f32`** at the original source rate — each is
/// directly usable as a `DeckBuffer`'s `samples`.
#[derive(Debug, Clone, Default)]
pub struct StemSet {
    pub drums: Vec<f32>,
    pub bass: Vec<f32>,
    pub other: Vec<f32>,
    pub vocals: Vec<f32>,
}

impl StemSet {
    /// Stems in canonical [`SOURCES`] order: `[drums, bass, other, vocals]`.
    pub fn in_order(&self) -> [&Vec<f32>; 4] {
        [&self.drums, &self.bass, &self.other, &self.vocals]
    }
}

/// Map any `ort::Error<C>` (generic over its context type) into our string-backed variant.
fn ort_err<E: std::fmt::Display>(e: E) -> StemError {
    StemError::Ort(e.to_string())
}

/// A loaded htdemucs ONNX session. Construct once (loading is expensive), then reuse across tracks.
pub struct StemSeparator {
    session: Session,
}

impl StemSeparator {
    /// Load a single-file htdemucs ONNX model from disk.
    pub fn load(model_path: &Path) -> Result<Self, StemError> {
        let session = Session::builder()
            .map_err(ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?
            .commit_from_file(model_path)
            .map_err(ort_err)?;
        Ok(Self { session })
    }

    /// Separate an **interleaved stereo** mix into four stems at the same sample rate.
    ///
    /// `progress` is called after each chunk with a fraction in `0.0..=1.0`. Offline / blocking;
    /// run it on a worker thread. Errors if `source_rate != MODEL_SAMPLE_RATE` (S1 limitation).
    pub fn separate(
        &mut self,
        mix_interleaved: &[f32],
        source_rate: u32,
        progress: impl FnMut(f32),
    ) -> Result<StemSet, StemError> {
        if source_rate != MODEL_SAMPLE_RATE {
            return Err(StemError::UnsupportedRate {
                got: source_rate,
                expected: MODEL_SAMPLE_RATE,
            });
        }
        if mix_interleaved.len() % 2 != 0 {
            return Err(StemError::NotStereo(mix_interleaved.len()));
        }
        let (left, right) = deinterleave(mix_interleaved);
        let stems = overlap_add(&left, &right, progress, |lc, rc| self.run_chunk(lc, rc))?;
        Ok(StemSet {
            drums: interleave(&stems[0][0], &stems[0][1]),
            bass: interleave(&stems[1][0], &stems[1][1]),
            other: interleave(&stems[2][0], &stems[2][1]),
            vocals: interleave(&stems[3][0], &stems[3][1]),
        })
    }

    /// Run one padded `N_SAMPLES` chunk through the model. `lc`/`rc` are `N_SAMPLES` each; returns
    /// the flat `[4][2][N_SAMPLES]` output (stem-major, then channel, then sample).
    fn run_chunk(&mut self, lc: &[f32], rc: &[f32]) -> Result<Vec<f32>, StemError> {
        debug_assert_eq!(lc.len(), N_SAMPLES);
        debug_assert_eq!(rc.len(), N_SAMPLES);
        let mut flat = vec![0.0f32; N_CHANNELS * N_SAMPLES];
        flat[..N_SAMPLES].copy_from_slice(lc);
        flat[N_SAMPLES..].copy_from_slice(rc);

        let input = Tensor::from_array(([1usize, N_CHANNELS, N_SAMPLES], flat)).map_err(ort_err)?;
        let outputs = self
            .session
            .run(ort::inputs!["mix" => input])
            .map_err(ort_err)?;
        let (_shape, data) = outputs["stems"]
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;

        let expected = SOURCES.len() * N_CHANNELS * N_SAMPLES;
        if data.len() != expected {
            return Err(StemError::BadOutput {
                got: data.len(),
                expected,
            });
        }
        Ok(data.to_vec())
    }
}

/// Triangular fade-in/fade-out window (length [`N_SAMPLES`]) used for overlap-add stitching: a
/// linear ramp `0..1` over the first [`OVERLAP`] samples, flat `1.0` in the middle, mirrored ramp
/// at the tail. Matches `np.linspace(0, 1, OVERLAP)` and its reverse.
fn transition_window() -> Vec<f32> {
    let mut w = vec![1.0f32; N_SAMPLES];
    let denom = (OVERLAP - 1).max(1) as f32;
    for i in 0..OVERLAP {
        let v = i as f32 / denom;
        w[i] = v;
        w[N_SAMPLES - 1 - i] = v;
    }
    w
}

/// Number of chunks needed to cover `total_len` samples at [`STRIDE`] hop (min 1).
fn n_chunks(total_len: usize) -> usize {
    if total_len == 0 {
        return 1;
    }
    total_len.div_ceil(STRIDE).max(1)
}

/// Generic segmented overlap-add: slices the planar `left`/`right` mix into [`N_SAMPLES`] chunks
/// (zero-padded at the tail), runs `run_chunk` on each padded chunk, windows + accumulates the
/// four returned stems, then normalizes by the summed window weight. Factored out (no `ort`
/// dependency) so the stitching is unit-testable with a stub model.
fn overlap_add<F>(
    left: &[f32],
    right: &[f32],
    mut progress: impl FnMut(f32),
    mut run_chunk: F,
) -> Result<[[Vec<f32>; N_CHANNELS]; 4], StemError>
where
    F: FnMut(&[f32], &[f32]) -> Result<Vec<f32>, StemError>,
{
    let total_len = left.len();
    let window = transition_window();
    let chunks = n_chunks(total_len);

    let mut out: [[Vec<f32>; N_CHANNELS]; 4] =
        std::array::from_fn(|_| std::array::from_fn(|_| vec![0.0f32; total_len]));
    let mut weight = vec![0.0f32; total_len];

    let mut lc = vec![0.0f32; N_SAMPLES];
    let mut rc = vec![0.0f32; N_SAMPLES];

    for i in 0..chunks {
        let start = i * STRIDE;
        if start >= total_len {
            break;
        }
        let end = (start + N_SAMPLES).min(total_len);
        let clen = end - start;

        lc.iter_mut().for_each(|s| *s = 0.0);
        rc.iter_mut().for_each(|s| *s = 0.0);
        lc[..clen].copy_from_slice(&left[start..end]);
        rc[..clen].copy_from_slice(&right[start..end]);

        let res = run_chunk(&lc, &rc)?; // flat [4][2][N_SAMPLES]
        for (stem, stem_out) in out.iter_mut().enumerate() {
            for (ch, chan_out) in stem_out.iter_mut().enumerate() {
                let base = (stem * N_CHANNELS + ch) * N_SAMPLES;
                let dst = &mut chan_out[start..end];
                for k in 0..clen {
                    dst[k] += res[base + k] * window[k];
                }
            }
        }
        for k in 0..clen {
            weight[start + k] += window[k];
        }
        progress((i + 1) as f32 / chunks as f32);
    }

    for (k, &w) in weight.iter().enumerate() {
        let wk = w.max(1e-8);
        for stem_out in out.iter_mut() {
            for chan_out in stem_out.iter_mut() {
                chan_out[k] /= wk;
            }
        }
    }
    Ok(out)
}

/// Split interleaved stereo `[l0, r0, l1, r1, …]` into planar `(left, right)`.
fn deinterleave(stereo: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let frames = stereo.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for f in 0..frames {
        left.push(stereo[2 * f]);
        right.push(stereo[2 * f + 1]);
    }
    (left, right)
}

/// Merge planar `(left, right)` back into interleaved stereo.
fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    let n = left.len().min(right.len());
    let mut out = Vec::with_capacity(n * 2);
    for f in 0..n {
        out.push(left[f]);
        out.push(right[f]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_shape_is_triangular() {
        let w = transition_window();
        assert_eq!(w.len(), N_SAMPLES);
        assert!((w[0] - 0.0).abs() < 1e-6, "fades in from 0");
        assert!((w[N_SAMPLES - 1] - 0.0).abs() < 1e-6, "fades out to 0");
        assert!(
            (w[N_SAMPLES / 2] - 1.0).abs() < 1e-6,
            "flat 1.0 in the middle"
        );
        assert!(
            w[OVERLAP / 2] > 0.0 && w[OVERLAP / 2] < 1.0,
            "ramps within the fade"
        );
    }

    #[test]
    fn chunk_count_covers_signal() {
        assert_eq!(n_chunks(0), 1);
        assert_eq!(n_chunks(1), 1);
        assert_eq!(n_chunks(N_SAMPLES), 2); // first chunk + overlap tail
        assert_eq!(n_chunks(STRIDE), 1);
        assert_eq!(n_chunks(STRIDE + 1), 2);
    }

    /// With an identity "model" (each stem == the input chunk), overlap-add + weight
    /// normalization must reconstruct the original signal across chunk boundaries.
    #[test]
    fn overlap_add_reconstructs_with_identity_model() {
        // ~2.5 segments so we exercise multiple overlaps and the padded tail.
        let total = STRIDE * 2 + 12_345;
        let left: Vec<f32> = (0..total).map(|i| (i as f32 * 0.001).sin()).collect();
        let right: Vec<f32> = (0..total).map(|i| (i as f32 * 0.002).cos()).collect();

        let mut calls = 0usize;
        let out = overlap_add(
            &left,
            &right,
            |_| {},
            |lc, rc| {
                calls += 1;
                // Identity model: every stem returns the input chunk unchanged.
                let mut flat = vec![0.0f32; 4 * N_CHANNELS * N_SAMPLES];
                for stem in 0..4 {
                    let lbase = (stem * N_CHANNELS) * N_SAMPLES;
                    let rbase = (stem * N_CHANNELS + 1) * N_SAMPLES;
                    flat[lbase..lbase + N_SAMPLES].copy_from_slice(lc);
                    flat[rbase..rbase + N_SAMPLES].copy_from_slice(rc);
                }
                Ok(flat)
            },
        )
        .unwrap();

        assert_eq!(calls, n_chunks(total));
        // The triangular window is exactly 0 at the very first sample of the first chunk (nothing
        // overlaps it), so that single endpoint normalizes to ~0 — a known, negligible edge that
        // the reference demucs-onnx shares. Verify exact reconstruction across the interior.
        for stem in &out {
            for (ch, want) in [&left, &right].into_iter().enumerate() {
                let got = &stem[ch];
                for k in 1..total - 1 {
                    assert!(
                        (got[k] - want[k]).abs() < 1e-4,
                        "stem ch{ch}[{k}] {} vs {}",
                        got[k],
                        want[k]
                    );
                }
            }
        }
    }

    /// Live model smoke test — proves the Rust `ort` path can load the real htdemucs graph and
    /// run one `[1,2,343980]` frame to `[1,4,2,343980]`. Ignored by default (CI has no 301 MB
    /// model); run locally with the model path in `COMPAS_HTDEMUCS_ONNX`, e.g.
    /// `COMPAS_HTDEMUCS_ONNX=~/.cache/huggingface/.../htdemucs.onnx cargo test -p compas-stems -- --ignored`.
    #[test]
    #[ignore = "requires the 301 MB htdemucs.onnx via COMPAS_HTDEMUCS_ONNX"]
    fn ort_smoke_runs_real_model() {
        let path = std::env::var("COMPAS_HTDEMUCS_ONNX")
            .expect("set COMPAS_HTDEMUCS_ONNX to the htdemucs.onnx path");
        let mut sep = StemSeparator::load(Path::new(&path)).expect("load model");
        let silence = vec![0.0f32; N_SAMPLES]; // one channel of a padded chunk
        let out = sep.run_chunk(&silence, &silence).expect("run one frame");
        assert_eq!(out.len(), SOURCES.len() * N_CHANNELS * N_SAMPLES);
    }
}
