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

### 3. Half/double tempo scoring — DONE

Track octave-related candidates explicitly and pick the musically plausible one using onset support
and confidence, not only the largest autocorrelation peak.

Acceptance:

- A promoted half/double fixture is active, not ignored.
- 90/120/128/150 BPM regression cases remain within tolerance.
- Benchmarks do not show an unreasonable regression for offline full-track analysis.

Landed as `TempoAnalysis::select_tempo` + `tempo_prior` (see
`docs/research/summaries/half-double-tempo-scoring.md`). Among the winning lag and its ½×/2×
octaves, the pick maximizes `onset_support × tempo_prior(bpm)`, where the prior is a broad
log-normal resonance peaking near a danceable tempo (used only to break genuine 2:1 ties, never to
invent a tempo). `beat_tracking_resolves_half_double_tempo_trap` is now active;
`octave_scoring_lifts_accent_trap_to_dance_tempo` shows the teeth (raw peak 75 BPM → resolved 150);
`beat_evaluation_matrix` keeps 90/120/128/150 within tolerance and promotes `half_double_trap` +
`accent_trap_150` to Solid. The diagnostics path uses the same `select_tempo`, so `selected_bpm`
stays in lockstep with the public API (and may now be an octave of `candidates[selected]`).

### 4. Sparse-intro weighting — DONE

Reduce the influence of isolated intro hits when a later steady region provides stronger periodic
evidence.

Acceptance:

- Sparse-intro fixture remains active.
- A new sparse-intro variant with misleading early hits passes.
- Beatgrid phase still lands near the first real steady beat or reports lower confidence.

Landed as `apply_density_weight` (called in `analyze_tempo`; see
`docs/research/summaries/sparse-intro-weighting.md`). Each onset-envelope sample is scaled by a
local onset *rate* (a moving average of a saturating onset presence `env/(env+mean_env)`, so loud
hits don't read as "busy"), via `w = act/(act + 0.5·mean_act)`. Uniformly-active material is scaled
by a constant → tempo peak and comb phase unchanged; only below-average-rate regions (sparse intros,
breakdowns) are suppressed. New teeth test `beatgrid_resists_loud_sparse_intro` is pulled 0.224 s off
the groove with the weighting disabled and locks on with it enabled; `misleading_sparse_124` is
promoted Reference → Solid; `estimate_tempo_8s` benchmark shows no measurable change. Value-only — no
public `TempoEstimate`/`BeatGrid` change.

### 5. Online/live-input tracking — DESIGN NOTE DONE; implementation gated

Do not mix this into offline local-file analysis. If OBTAIN or a verified zero-latency source is
adopted, it should live behind a separate live-input design because it trades future context for
causality.

Acceptance:

- Separate design note first. ✅ — `docs/research/live-input-beat-tracking.md` (goal/non-goals, RT
  boundary, OBTAIN-style causal algorithm, the virtual-leader sync contract with the existing deck
  PLL + the shipped `compas-audio::input` aux capture, latency budget, phased plan, streaming-chunk
  test plan).
- No changes to local-file beatgrid quality without offline benchmarks. ▶ enforced per slice — the
  planned `LiveTracker` is additive and must leave `estimate_*` + the matrix/benches untouched.
- Explicit routing/sync contract for mic, aux, external gear, or streaming control-only timing. ✅
  (design note §5).

**Next implementation slice (stop-for-review):** the pure `LiveTracker` core in `compas-dsp`
(sliding-window onset→tempo→phase, allocation-free, no engine wiring) with the streaming-chunk
harness — zero engine surface, proves the algorithm on synthetic streams before any RT plumbing.

## First implementation decision

The next code slice should be **candidate tempo diagnostics**, followed by **confidence
calibration**. That sequence gives the app visibility into ambiguous grids before changing tempo
selection behavior, and it has a small rollback surface.
