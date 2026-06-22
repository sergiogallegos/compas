# OBTAIN: Real-Time Beat Tracking in Audio Signals

## Citation

- Authors: Ali Mottaghi, Kayhan Behdin, Ashkan Esmaeili, Mohammadreza Heydari, Farokh Marvasti
- Year: 2017
- Link: https://arxiv.org/abs/1704.02216
- Local file: `docs/research/papers/mottaghi-obtain-2017.pdf` (ignored)
- License / redistribution: arXiv-hosted PDF. Do not commit the PDF unless redistribution terms are
  checked separately.

## Core ideas

- OBTAIN is an online beat-tracking method aimed at real-time audio.
- The pipeline is onset-strength extraction, tempo estimation, cumulative beat-strength scoring,
  then peak selection over periodic beat candidates.
- The paper frames BeatRoot/IBT-style systems as prior real-time/offline references and tries to
  keep computational cost practical enough for embedded hardware.
- This is useful for compas as a causal/live-input architecture, not automatically as an offline
  local-file replacement.

## Current compas behavior

- `compas-dsp::analysis` does offline local-file analysis from decoded PCM.
- The estimator can use future context because tracks are already loaded in RAM.
- The public app contract is a tempo estimate plus beatgrid, not an online stream of beat events.
- There is no mic/aux/live-input beat follower yet.

## Useful changes

- Keep OBTAIN as the fallback source if the requested 2024 zero-latency citation remains
  unverified.
- Add a future `live-input-beat-tracking.md` design before any online tracker enters the engine.
- Reuse the idea of explicit candidate scoring for offline diagnostics: candidate tempi,
  cumulative support, and confidence should be inspectable in tests before selection changes.
- Add synthetic online-oriented tests later: cold start, tempo change, missed onsets, false onset
  bursts, and no-lookahead behavior.

## Risks / non-goals

- Do not put online beat tracking in the cpal callback. Even a causal tracker needs buffering,
  allocation-free state design, and a separate timing contract.
- Do not replace offline analysis with OBTAIN just because it is real-time; local files can afford
  offline context and should use that advantage.
- Do not add a live-input sync feature without routing, latency, and clock-domain design.

## Tests or benchmarks required

- `cargo test -p compas-dsp --locked` for all estimator changes.
- `cargo bench -p compas-dsp` for non-trivial analysis changes.
- A future live-input harness should simulate streaming chunks and verify bounded per-chunk cost.
- Confidence tests should prove ambiguous candidate sets do not look as reliable as clean grids.

## Decision

Revisit after offline diagnostics and confidence calibration. OBTAIN is a valid reference for
future live input, mic/aux, or external-gear tracking, but the next compas beat-tracking work should
remain offline: candidate tempo diagnostics first, then confidence calibration.
