# Beat-tracking literature verification

## Citation status

| Requested source | Verification status | Notes |
|---|---|---|
| Dixon (2000), "A beat tracking system for audio signals" | **Not verified under that exact title** | The accessible trail points to Simon Dixon's BeatRoot work. A secondary BeatRoot page describes a tempo process plus beat-synchronization process with multiple agents. Need primary paper/PDF before detailed implementation claims. |
| Laroche (2003), "Efficient Tempo and Beat Tracking" | **Not verified** | Exact-title and author searches did not return a reliable source in this pass. Need DOI, venue, or corrected title. |
| Mierer et al. (2024), "A real-time beat tracking system with zero latency" | **Not verified** | Exact-title and broad zero-latency searches did not find this paper. Possible misspelling or different title. |
| Mottaghi et al. (2017), "OBTAIN: Real-Time Beat Tracking in Audio Signals" | **Downloaded and summarized** | arXiv source found. Uses onset strength, tempo estimation, cumulative beat strength, and peak selection for online beat tracking. See `obtain-online-beat-tracking.md`. |
| Foscarin, Schlueter, Widmer (2024), "Beat this! Accurate beat tracking without DBN postprocessing" | **Downloaded and summarized** | Not the requested zero-latency paper, but a useful modern beat/downbeat evaluation source. See `beat-this-2024.md`. |

Known links:

- BeatRoot overview: https://en.wikipedia.org/wiki/BeatRoot
- OBTAIN arXiv: https://arxiv.org/abs/1704.02216
- Beat this arXiv: https://arxiv.org/abs/2407.21658

## Useful ideas for compas

### Dixon / BeatRoot trail

Use as a design direction, not yet as a source for detailed algorithmic claims:

- Keep tempo induction and beat synchronization conceptually separate.
- Maintain multiple candidate hypotheses instead of committing too early to one tempo/phase.
- Track half-time/double-time alternatives explicitly.
- Prefer a confidence score that the UI/planner can use instead of treating every detected
  beatgrid as equally reliable.

Potential compas outputs:

- synthetic tests for half/double tempo traps;
- beatgrid confidence field;
- "candidate tempo set" in offline analysis for debugging;
- sync edge-case tests where the follower should not snap to a bad leader grid.

### Laroche trail

Because the citation is not verified yet, do not implement anything under Laroche's name. If the
paper is found, evaluate it against:

- CPU cost on full-track offline analysis;
- robustness to EDM kicks vs sparse intros;
- handling of tempo ramps;
- half/double tempo disambiguation;
- whether it improves over the current spectral-flux/autocorrelation path.

### Zero-latency 2024 trail

Because the citation is not verified, treat this as an open research task. If found, decide whether
it belongs in compas at all:

- Local file analysis can use future context, so zero-latency is not mandatory there.
- Zero-latency online tracking matters more for live input, mic/aux, external gear, or streaming
  control-only timing.
- If the method trades accuracy for causality, it should not replace offline beatgrid analysis for
  loaded local files without benchmarks.

### OBTAIN fallback

OBTAIN is the verified online/real-time fallback source for now. The useful architecture is:

- compute onset strength;
- estimate tempo candidates;
- accumulate beat-strength evidence over time;
- select periodic peaks as beat candidates.

Potential compas outputs:

- online/live-input roadmap note;
- beat-tracking benchmark cases before any algorithm swap;
- compare OBTAIN-style candidate scoring against current offline analysis.

### Beat this 2024

This is not a zero-latency tracker and should not be treated as a direct implementation target for
the current Rust DSP path. Its value is the evaluation warning:

- fixed tempo/meter postprocessing assumptions can improve common-case continuity while failing
  harder music;
- a method can improve F1 score while degrading continuity metrics;
- compas should measure confidence and grid continuity before adopting more aggressive candidate
  selection or any model-based estimator.

## Current compas behavior

- `compas-dsp::analysis` already performs offline BPM/beatgrid/key analysis on local decoded PCM.
- Sync uses a beat interval and beat offset once a deck is loaded.
- There is no explicit confidence score or candidate tempo set exposed today.
- Current sync tests cover phase lock and tempo-only behavior, but not the full half/double tempo,
  bad leader, silence, sparse intro, and tempo-ramp matrix.

## Decision

Do not rewrite beat tracking yet. First build the benchmark/test harness from point 4, then compare
current analysis against any Dixon/Laroche/OBTAIN/Beat-this-inspired changes. The immediate
implementation value is stronger tests, candidate diagnostics, confidence, and continuity
measurement, not a new algorithm.

## Follow-up searches

- Search by Dixon/BeatRoot paper bibliography, not just the requested title.
- Ask for or find the exact Laroche venue/DOI.
- Re-check whether "Mierer" is misspelled: try Meier, Meyer, MIREX, MIRE, MIREX 2024, ISMIR 2024,
  and "causal beat tracking" instead of "zero latency".
