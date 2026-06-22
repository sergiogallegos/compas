//! Real-track beat-tracking evaluation (research-backed TODO 4).
//!
//! Synthetic click tracks prove the estimator is *correct* on idealized input; a real-track
//! corpus is how we measure whether it is *accurate* on music. Because that audio is usually
//! copyrighted, the corpus is kept out of git (see `crates/compas-dsp/eval/README.md`) and
//! this test only runs when pointed at a local manifest:
//!
//! ```text
//! COMPAS_BEAT_EVAL=eval/manifest.csv cargo test -p compas-dsp \
//!     --test beat_real_track_eval -- --nocapture
//! ```
//!
//! (Relative manifest paths resolve against the crate dir, since that is the working directory
//! cargo gives test binaries; an absolute path also works.)
//!
//! With no manifest the test prints a skip line and passes, so CI and fresh clones stay green.
//! It is report-first: it never fails on accuracy unless `COMPAS_BEAT_EVAL_STRICT=1`, in which
//! case it asserts the exact-match rate is at least `COMPAS_BEAT_EVAL_MIN` (default 0.8).

use std::path::{Path, PathBuf};

use compas_dsp::analysis::estimate_tempo;

/// Minimal canonical-WAV reader → (mono samples, sample_rate). Supports PCM 8/16/24/32-bit
/// and IEEE float 32, including WAVE_FORMAT_EXTENSIBLE, which covers anything `ffmpeg -f wav`
/// emits. Returns `None` on anything it does not understand rather than guessing.
fn read_wav_mono(path: &Path) -> Option<(Vec<f32>, u32)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16le = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]);
    let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);

    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (format, channels, rate, bits)
    let mut data: Option<(usize, usize)> = None; // (offset, len)
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32le(&bytes[pos + 4..pos + 8]) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            break;
        }
        if id == b"fmt " && size >= 16 {
            let mut format = u16le(&bytes[body..]);
            let channels = u16le(&bytes[body + 2..]);
            let rate = u32le(&bytes[body + 4..]);
            let bits = u16le(&bytes[body + 14..]);
            // WAVE_FORMAT_EXTENSIBLE: the real format is the first 2 bytes of the sub-format GUID.
            if format == 0xFFFE && size >= 26 {
                format = u16le(&bytes[body + 24..]);
            }
            fmt = Some((format, channels, rate, bits));
        } else if id == b"data" {
            data = Some((body, size));
        }
        pos = body + size + (size & 1); // chunks are word-aligned
    }

    let (format, channels, rate, bits) = fmt?;
    let (off, len) = data?;
    let ch = channels.max(1) as usize;
    let bytes_per_sample = (bits / 8).max(1) as usize;
    let frame_bytes = bytes_per_sample * ch;
    if frame_bytes == 0 {
        return None;
    }
    let raw = &bytes[off..off + len];

    let decode = |s: &[u8]| -> Option<f32> {
        match (format, bits) {
            (1, 8) => Some((s[0] as f32 - 128.0) / 128.0),
            (1, 16) => Some(i16::from_le_bytes([s[0], s[1]]) as f32 / 32_768.0),
            (1, 24) => {
                let v = (s[0] as i32) | ((s[1] as i32) << 8) | ((s[2] as i32) << 16);
                let v = (v << 8) >> 8; // sign-extend 24 -> 32
                Some(v as f32 / 8_388_608.0)
            }
            (1, 32) => Some(i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0),
            (3, 32) => Some(f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
            _ => None,
        }
    };

    let mut mono = Vec::with_capacity(raw.len() / frame_bytes);
    for frame in raw.chunks_exact(frame_bytes) {
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += decode(&frame[c * bytes_per_sample..])?;
        }
        mono.push(sum / ch as f32);
    }
    Some((mono, rate))
}

/// Resolve the manifest path. Cargo runs test binaries with the **crate dir** as the working
/// directory, so a bare relative path is tried as given and then against `CARGO_MANIFEST_DIR`,
/// which is what lets `eval/manifest.csv` work regardless of where `cargo test` was launched.
fn resolve_manifest(raw: &str) -> PathBuf {
    let given = PathBuf::from(raw);
    if given.is_absolute() || given.is_file() {
        return given;
    }
    let crate_relative = Path::new(env!("CARGO_MANIFEST_DIR")).join(&given);
    if crate_relative.is_file() {
        return crate_relative;
    }
    given
}

struct Entry {
    path: PathBuf,
    expected_bpm: f32,
}

/// Parse a `path,expected_bpm` manifest. Blank lines and `#` comments are ignored; WAV paths
/// are resolved relative to the manifest's own directory.
fn parse_manifest(manifest: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(manifest).unwrap_or_default();
    let base = manifest.parent().unwrap_or_else(|| Path::new("."));
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((p, bpm)) = line.rsplit_once(',') else {
            continue;
        };
        let Ok(expected_bpm) = bpm.trim().parse::<f32>() else {
            continue;
        };
        let rel = Path::new(p.trim());
        let path = if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            base.join(rel)
        };
        entries.push(Entry { path, expected_bpm });
    }
    entries
}

/// True if `got` matches `expected` within `tol` at the same tempo or an octave (½×/2×) of it.
fn octave_match(got: f32, expected: f32, tol: f32) -> (bool, bool) {
    let exact = (got - expected).abs() <= tol;
    let octave =
        exact || (got - expected * 2.0).abs() <= tol || (got - expected * 0.5).abs() <= tol;
    (exact, octave)
}

#[test]
fn real_track_beat_evaluation() {
    let Ok(manifest) = std::env::var("COMPAS_BEAT_EVAL") else {
        eprintln!(
            "real-track eval skipped: set COMPAS_BEAT_EVAL=<manifest.csv> to run \
             (see crates/compas-dsp/eval/README.md)"
        );
        return;
    };
    let manifest = resolve_manifest(&manifest);
    assert!(
        manifest.is_file(),
        "COMPAS_BEAT_EVAL points at a missing file: {} \
         (relative paths resolve against the crate dir, {})",
        manifest.display(),
        env!("CARGO_MANIFEST_DIR")
    );
    let entries = parse_manifest(&manifest);
    assert!(
        !entries.is_empty(),
        "COMPAS_BEAT_EVAL={} has no usable `path,bpm` rows",
        manifest.display()
    );

    let tol: f32 = std::env::var("COMPAS_BEAT_EVAL_TOL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2.0);

    let (mut exact_hits, mut octave_hits, mut scored) = (0u32, 0u32, 0u32);
    let mut missing = 0u32;
    eprintln!(
        "\n{:<40} {:>8} {:>8} {:>6}  result",
        "track", "expect", "got", "conf"
    );
    for e in &entries {
        let Some((mono, rate)) = read_wav_mono(&e.path) else {
            eprintln!(
                "{:<40} {:>8.1}  (unreadable WAV, skipped)",
                e.path.display(),
                e.expected_bpm
            );
            missing += 1;
            continue;
        };
        let est = estimate_tempo(&mono, rate);
        let (exact, octave) = octave_match(est.bpm, e.expected_bpm, tol);
        scored += 1;
        if exact {
            exact_hits += 1;
        }
        if octave {
            octave_hits += 1;
        }
        let name = e
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        eprintln!(
            "{:<40} {:>8.1} {:>8.2} {:>6.3}  {}",
            name,
            e.expected_bpm,
            est.bpm,
            est.confidence,
            if exact {
                "EXACT"
            } else if octave {
                "octave"
            } else {
                "MISS"
            }
        );
    }

    let exact_rate = if scored > 0 {
        exact_hits as f32 / scored as f32
    } else {
        0.0
    };
    let octave_rate = if scored > 0 {
        octave_hits as f32 / scored as f32
    } else {
        0.0
    };
    eprintln!(
        "\nscored {scored} tracks ({missing} unreadable): exact {exact_hits}/{scored} \
         ({:.0}%), within-octave {octave_hits}/{scored} ({:.0}%)\n",
        exact_rate * 100.0,
        octave_rate * 100.0
    );

    if std::env::var("COMPAS_BEAT_EVAL_STRICT").is_ok() {
        let min: f32 = std::env::var("COMPAS_BEAT_EVAL_MIN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.8);
        assert!(
            exact_rate >= min,
            "exact match rate {exact_rate:.2} below required {min:.2}"
        );
    }
}
