# Real-track beat-tracking evaluation corpus

This directory holds a **local-only** corpus for measuring the beat tracker against real music.
The audio is usually copyrighted, so everything here except this README and `.gitignore` is
git-ignored — nothing in your corpus is ever committed.

The synthetic harness (`tests/beat_tracking_harness.rs`) proves the estimator is *correct* on
idealized clicks and tracks the known-gap matrix. This corpus is how we measure whether it is
*accurate* on real tracks, where onset envelopes are messy and half/double ambiguity is real.

## Setup

1. Drop WAV files into this directory (or any local path). Convert from other formats with, e.g.:

   ```bash
   ffmpeg -i track.flac -ac 2 -ar 44100 track.wav
   ```

   The reader supports PCM 8/16/24/32-bit and IEEE float 32, mono or multi-channel (it downmixes).

2. Create a `manifest.csv` listing each file and its known BPM (one per line, `#` comments and
   blank lines allowed). Paths are resolved relative to the manifest:

   ```text
   # filename, expected_bpm
   track-a.wav, 128
   track-b.wav, 124
   subdir/track-c.wav, 100
   ```

## Run

```bash
COMPAS_BEAT_EVAL=eval/manifest.csv \
  cargo test -p compas-dsp --test beat_real_track_eval -- --nocapture
```

Relative manifest paths resolve against this crate's directory (the working directory cargo
gives test binaries), so `eval/manifest.csv` works no matter where you launch `cargo test`; an
absolute path works too. The test prints a per-track table plus exact-match and within-octave
rates. With no `COMPAS_BEAT_EVAL` set it prints a skip line and passes, so CI and fresh clones
stay green.

### Options

| Env var                    | Default | Effect                                                        |
| -------------------------- | ------- | ------------------------------------------------------------- |
| `COMPAS_BEAT_EVAL`         | (unset) | Path to the manifest. Unset → skip.                           |
| `COMPAS_BEAT_EVAL_TOL`     | `2.0`   | BPM tolerance for an exact match.                             |
| `COMPAS_BEAT_EVAL_STRICT`  | (unset) | If set, fail the test when the exact rate is below the floor. |
| `COMPAS_BEAT_EVAL_MIN`     | `0.8`   | Exact-match rate floor used in strict mode.                   |

The within-octave rate is reported separately so half/double misses (the current known gap, see
`docs/research/beat-tracking-adoption-plan.md`) are visible without masking real failures.
