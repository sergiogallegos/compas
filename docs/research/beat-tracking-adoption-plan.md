# Beat-tracking adoption plan

This is the gate for changing `compas-dsp::analysis` after the initial literature intake and
synthetic harness. The goal is to improve the beat tracker in small, reversible slices instead of
landing a large algorithm rewrite.

## Current baseline

- `estimate_tempo` uses a spectral-flux onset envelope plus autocorrelation over 70-180 BPM.
- `estimate_beatgrid` reuses that tempo and combs candidate phases over the onset envelope.
- The implementation is offline only. It allocates and must stay outside the cpal callback.
- `crates/compas-dsp/tests/beat_tracking_harness.rs` covers common dance tempos, delayed first
  beat phase, and sparse intros. It also keeps harder tempo-ramp, half/double, and swung-drum cases
  as ignored reference fixtures.
- `cargo bench -p compas-dsp` includes `estimate_tempo_8s` and `estimate_beatgrid_12s`.

## Adoption gate

Every beat-tracking algorithm change needs all of the following before it lands:

1. **Source note:** a short summary in `docs/research/summaries/` describing the paper/talk/code
   source, what is verified, and what is only inferred.
2. **Target behavior:** one concrete failure mode, not a general "better BPM" claim. Examples:
   half/double tempo ambiguity, sparse intro drift, swung subdivisions, tempo ramp averaging, or
   low-confidence silence/noise.
3. **Tests first:** a failing or ignored harness case promoted to an active test, plus any new
   fixture needed to describe the behavior.
4. **Cost check:** run `cargo test -p compas-dsp --locked`; run `cargo bench -p compas-dsp` for
   non-trivial analysis changes and record the before/after direction in the commit message or
   changelog.
5. **UI contract:** preserve the public `TempoEstimate` and `BeatGrid` fields unless the app UI,
   sync planner, and persisted metadata are updated in the same slice.
6. **Real-time boundary:** no beat-tracking code moves onto the audio thread. Online/live-input
   tracking must have its own design note before implementation.
7. **Rollback path:** keep the old behavior isolated enough that one commit can revert the change
   without touching unrelated DSP, sync, or UI code.

## Candidate slices

### 1. Candidate tempo diagnostics — DONE

Expose internal tempo candidates in test-only or debug-only code so half/double decisions can be
seen instead of guessed. This should not change the public app contract.

Acceptance:

- Active tests still pass.
- Ignored half/double fixture prints or asserts candidate ranking in a local debug path.
- No public API change unless the UI is updated to consume candidate confidence.

Landed as `estimate_tempo_diagnostics` (additive; shared `analyze_tempo` core keeps it in lockstep
with the estimator). `diagnostics_expose_half_double_candidates` asserts the 64 BPM octave's support
is visible while the estimator still picks 128.

### 2. Confidence calibration — DONE

Make `TempoEstimate.confidence` and `BeatGrid.confidence` reflect ambiguity more honestly. A track
with competing half/double candidates should not look as trustworthy as a clean click track.

Acceptance:

- Existing clean click fixtures keep non-zero confidence.
- New ambiguous fixture has lower confidence than the clean fixture.
- Sync and automix can treat low confidence as "verify grid" without changing deck playback.

Landed in `TempoAnalysis::confidence()`: periodic strength (saturating map of `best_r`, the fraction
of onset energy that repeats — this is what collapses confidence for noise/silence/weak onsets, which
peak prominence alone could not because the max of many near-zero lags still towers over their mean) ×
octave factor (half/double discount) × rival factor (competing in-range tempo). `estimate_beatgrid`
further scales by phase sharpness from the comb. Measured: clean clicks ~0.56-0.62, half/double trap
~0.27, noise ~0.00. Value-only — no struct/IPC/UI change; `bpm_confidence` is stored but not yet
gated on, so nothing downstream changes behavior.

### 3. Half/double tempo scoring

Track octave-related candidates explicitly and pick the musically plausible one using onset support
and confidence, not only the largest autocorrelation peak.

Acceptance:

- A promoted half/double fixture is active, not ignored.
- 90/120/128/150 BPM regression cases remain within tolerance.
- Benchmarks do not show an unreasonable regression for offline full-track analysis.

### 4. Sparse-intro weighting

Reduce the influence of isolated intro hits when a later steady region provides stronger periodic
evidence.

Acceptance:

- Sparse-intro fixture remains active.
- A new sparse-intro variant with misleading early hits passes.
- Beatgrid phase still lands near the first real steady beat or reports lower confidence.

### 5. Online/live-input tracking

Do not mix this into offline local-file analysis. If OBTAIN or a verified zero-latency source is
adopted, it should live behind a separate live-input design because it trades future context for
causality.

Acceptance:

- Separate design note first.
- No changes to local-file beatgrid quality without offline benchmarks.
- Explicit routing/sync contract for mic, aux, external gear, or streaming control-only timing.

## First implementation decision

The next code slice should be **candidate tempo diagnostics**, followed by **confidence
calibration**. That sequence gives the app visibility into ambiguous grids before changing tempo
selection behavior, and it has a small rollback surface.
