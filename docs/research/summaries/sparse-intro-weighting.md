# Sparse-intro weighting — source note

Adoption-plan slice 4. This note covers the local onset-density weighting added to
`compas-dsp::analysis` (`apply_density_weight`, applied inside `analyze_tempo`), per the gate in
`docs/research/beat-tracking-adoption-plan.md`.

## The failure mode

The tempo/phase stages score the whole onset envelope at once: autocorrelation sums products
across the track, and the downbeat comb sums envelope energy per candidate phase. A track that
opens with a few **isolated but loud** intro hits (a vocal stab, a riser, scattered percussion)
before the groove enters can have those hits dominate the *phase* comb — especially when they
happen to share a phase (e.g. spaced a whole number of beats apart) that is offset from the real
groove. The estimator then locks the downbeat onto the intro hits instead of the steady section.

Mild, truly-scattered intros were already handled (the steady section wins on summed energy —
`misleading_sparse_124` passed before this slice). The hard case is a small number of **very loud**
sparse hits: by raw energy they can outweigh a moderate groove and capture the phase.

## The change

Before autocorrelation/comb, scale each envelope sample by how *often* onsets occur locally:

```
sat[i]  = env[i] / (env[i] + mean_env)         # amplitude-robust onset presence in [0,1)
act[i]  = moving_average(sat, ~2 s window)      # local onset RATE, not energy
w[i]    = act[i] / (act[i] + 0.5 * mean_act)
env[i] *= w[i]
```

The key design point is the **saturation** in `sat`: a very loud hit and an ordinary onset both map
toward 1, so "density" reflects onset *rate*, not loudness. An earlier energy-based version was
fooled — a few loud hits read as a "busy" region and were not suppressed.

Because the weight depends only on local onset *rate*, a uniformly-active envelope (any clean,
steady track) has `act ≈ mean_act` everywhere → every sample scaled by the same constant → the
autocorrelation peak and the comb's argmax phase are **unchanged**. Only regions whose onset rate is
well below the track average (sparse intros, breakdowns) are attenuated.

## What is verified vs inferred

- **Verified (in this repo):** `beatgrid_resists_loud_sparse_intro` is a teeth test — three loud
  hits spaced four beats apart, half a beat off the groove. With the weighting the downbeat locks to
  the groove (phase error ≤ 0.05 s); with the weighting disabled it is pulled 0.224 s onto the intro
  hits. All Solid `beat_evaluation_matrix` cases stay within tolerance, and `misleading_sparse_124`
  is promoted Reference → Solid. `cargo bench` shows no measurable change to `estimate_tempo_8s`
  (≈ 5.45 ms; within ±2.5% noise).
- **Inferred / our tuning, NOT a cited result:** the idea of weighting onset observations by local
  reliability/salience is common in beat-tracking (e.g. confidence-weighted onsets in Davies &
  Plumbley, and the general "trust the steadier region" intuition). The specific saturating
  rate measure, the ~2 s window, and the `act/(act+0.5·mean_act)` shape are **our own choices**,
  tuned against the synthetic harness — not lifted from a paper.

## Gate checklist

- **Target behaviour:** one concrete failure mode (loud sparse intro capturing the downbeat). ✅
- **Tests first:** new teeth test fails with the weighting disabled, passes with it; existing sparse
  fixture stays active; `misleading_sparse_124` promoted to Solid. ✅
- **Cost check:** two extra O(n) passes (saturation + prefix-sum moving average); `estimate_tempo_8s`
  benchmark shows no measurable change. ✅
- **UI contract:** no public field/IPC/UI change — `TempoEstimate`/`BeatGrid` untouched. ✅
- **RT boundary:** offline only; nothing moves onto the audio thread. ✅
- **Rollback:** isolated to `apply_density_weight` + its single call site in `analyze_tempo`;
  removing the call restores the previous behaviour without touching unrelated code. ✅

## Follow-ups

- The weighting also attenuates mid-track breakdowns/quiet sections; that is desirable for a single
  global tempo/phase, but a future variable-grid model should revisit it per-segment.
- Validate against the real-track corpus (`crates/compas-dsp/eval/`) on tracks with long ambient
  intros, where the analysis window may not reach the groove at all (an app-level windowing concern,
  separate from this envelope weighting).
