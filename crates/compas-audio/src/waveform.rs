//! Waveform peak extraction for rendering. Offline (runs on a worker after decode).

/// Compute one peak (max absolute amplitude across both channels) per bin of
/// `bin_frames` source frames. The result is a compact envelope the frontend renders
/// on a WebGL canvas; it is sent once per load rather than streaming the whole buffer.
///
/// `samples` is interleaved stereo. A 5-minute track at `bin_frames = 512` yields
/// ~25k peaks (~100 KB) — cheap to ship over IPC and to draw.
pub fn compute_peaks(samples: &[f32], bin_frames: usize) -> Vec<f32> {
    let bin_frames = bin_frames.max(1);
    let frames = samples.len() / 2;
    if frames == 0 {
        return Vec::new();
    }
    let n_bins = frames.div_ceil(bin_frames);
    let mut peaks = Vec::with_capacity(n_bins);

    let mut frame = 0;
    while frame < frames {
        let end = (frame + bin_frames).min(frames);
        let mut peak = 0.0f32;
        for f in frame..end {
            let l = samples[f * 2].abs();
            let r = samples[f * 2 + 1].abs();
            peak = peak.max(l).max(r);
        }
        peaks.push(peak);
        frame = end;
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_peaks() {
        assert!(compute_peaks(&[], 512).is_empty());
    }

    #[test]
    fn peak_is_max_abs_in_bin() {
        // 4 frames, bin of 2 -> 2 peaks. frame amps (max of l/r): 0.2, 0.9, 0.3, 0.5
        let s = [0.2, 0.1, -0.9, 0.4, 0.3, 0.0, 0.1, 0.5];
        let peaks = compute_peaks(&s, 2);
        assert_eq!(peaks.len(), 2);
        assert!((peaks[0] - 0.9).abs() < 1e-6);
        assert!((peaks[1] - 0.5).abs() < 1e-6);
    }
}
