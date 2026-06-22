# Half/double tempo scoring — source note

Adoption-plan slice 3. This note covers the octave-disambiguation change in
`compas-dsp::analysis` (`TempoAnalysis::select_tempo` + `tempo_prior`), per the gate in
`docs/research/beat-tracking-adoption-plan.md`.

## The failure mode

The estimator picks the lag with the largest autocorrelation, then octave-folds the BPM into
`[MIN_BPM, MAX_BPM]` (70–180). The largest autocorrelation lobe is **not** always the
musically-correct octave:

- A track with strong half-note **accents** peaks at half the perceived tempo (e.g. a 150 BPM
  pulse accented every other beat peaks at 75 BPM).
- A busy eighth-note groove can peak at double the perceived tempo.

Folding only fixes octaves that fall outside the range; when both `T` and `2T` sit inside
`[70, 180]` (true tempos in 70–90 vs their 140–180 doubles), folding cannot choose, and the raw
peak wins by default — sometimes the wrong octave.

## The change

For the winning lag and its ½× and 2× octaves, score each candidate by

```
onset_support(candidate) × tempo_prior(folded_bpm)
```

and take the best. `onset_support` is the candidate lag's autocorrelation relative to the winning
peak (so an octave with no onsets cannot win — the pick stays evidence-driven). `tempo_prior` is a
broad log-normal "resonance" weight peaking near a comfortable beat-matching tempo, used **only**
to break genuine 2:1 ties toward the danceable octave — never to invent a tempo.

```
tempo_prior(bpm) = exp(-0.5 * (ln(bpm / 125) / 0.55)^2)
```

## What is verified vs inferred

- **Verified (in this repo):** the behaviour is pinned by tests. `octave_scoring_lifts_accent_trap_to_dance_tempo`
  shows the raw peak is 75 BPM while the resolved tempo is 150 BPM. `beat_evaluation_matrix` keeps
  90/120/128/150 and the other Solid cases within tolerance, and promotes `half_double_trap` +
  `accent_trap_150` to Solid. `beat_tracking_resolves_half_double_tempo_trap` (previously ignored)
  now passes.
- **Inferred / our tuning, NOT a cited result:** the *concept* of a perceptual preferred-tempo
  resonance around ~120 BPM is well established in the beat-tracking literature (commonly associated
  with Parncutt 1994, Moelants 2002 "preferred tempo", and the tempo-preference weighting used by
  Davies & Plumbley and Klapuri). The specific center (125 BPM), width (σ = 0.55 in log-tempo), and
  the multiply-by-support combination here are **our own choices**, tuned against the synthetic
  matrix, not lifted from any one paper. Exact citations remain unverified — see
  `docs/research/summaries/beat-tracking-literature.md`, which already tracks open citation gaps.

## Gate checklist

- **Target behaviour:** one concrete failure mode (half/double octave choice). ✅
- **Tests first:** promoted fixture is active; a new teeth test shows the correction. ✅
- **Cost check:** `select_tempo` adds 3 octave evaluations (autocorrelation at a handful of lags)
  to the offline path; see the CHANGELOG/commit for the measured `estimate_tempo` direction. ✅
- **UI contract:** no public field/IPC/UI change — `TempoEstimate`/`BeatGrid` are untouched. ✅
- **RT boundary:** offline only; nothing moves onto the audio thread. ✅
- **Rollback:** isolated to `select_tempo` + `tempo_prior`; reverting them restores the old
  "largest peak then fold" pick without touching unrelated code. ✅

## Follow-ups

- Validate the prior against the real-track corpus (`crates/compas-dsp/eval/`) before trusting it on
  genuinely slow material (e.g. 80–90 BPM hip-hop), where doubling would be wrong. The prior is
  deliberately broad to avoid over-doubling, but only a real corpus can confirm the balance.
- Consider triplet (⅓×/3×) candidates for swung material, tracked separately from this slice.
