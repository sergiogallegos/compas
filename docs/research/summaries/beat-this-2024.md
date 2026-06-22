# Beat this! Accurate beat tracking without DBN postprocessing

## Citation

- Authors: Francesco Foscarin, Jan Schlueter, Gerhard Widmer
- Year: 2024
- Venue: ISMIR 2024
- Link: https://arxiv.org/abs/2407.21658
- Local file: `docs/research/papers/foscarin-beat-this-2024.pdf` (ignored)
- License / redistribution: arXiv-hosted PDF. Do not commit the PDF unless redistribution terms are
  checked separately.

## Core ideas

- The paper proposes a modern beat/downbeat tracker that avoids the usual Dynamic Bayesian Network
  postprocessing step.
- Its motivation is relevant even if compas does not adopt a neural model: fixed tempo/meter
  assumptions can fail on tempo variation, unusual meter, and discontinuous or difficult audio.
- The paper reports stronger F1 accuracy but also warns about failures in underrepresented genres
  and weaker continuity metrics.
- That accuracy/continuity tradeoff matters for DJ software: a single high-scoring beat estimate is
  not enough if sync stability or grid continuity degrades.

## Current compas behavior

- The current offline estimator is lightweight and deterministic: spectral-flux onset envelope,
  autocorrelation tempo search, and phase combing for beatgrid.
- compas does not use DBN postprocessing or a neural beat/downbeat model.
- The existing harness already has ignored fixtures for tempo ramps, half/double ambiguity, and
  swung drums.

## Useful changes

- Treat continuity as a first-class test property when improving beat tracking.
- Add confidence diagnostics before choosing more aggressive candidate selection.
- Keep the offline estimator explainable and cheap unless a model-based path has a clear license,
  artifact-size, runtime, and cross-platform deployment story.
- Use this paper as evidence that benchmark metrics need more than one score: accuracy, continuity,
  genre coverage, and failure reporting should all be visible.

## Risks / non-goals

- Do not add a neural model to compas from this paper in the near term. It would add runtime,
  packaging, licensing, and model-update complexity that is not justified before the lightweight
  estimator is better characterized.
- Do not optimize only for one beat F-measure if the resulting grids are less stable for DJ sync.
- Do not assume DBN removal is itself the lesson for compas; compas currently has no DBN.

## Tests or benchmarks required

- Active tests for half/double ambiguity, sparse intros, tempo ramps, and low-confidence silence.
- A continuity-style assertion for beat intervals once candidate diagnostics exist.
- Benchmarks before/after any non-trivial estimator change.
- A manual real-track evaluation list kept out of git if audio is copyrighted.

## Decision

Adopt the evaluation lesson, not the model. The next implementation should add candidate tempo
diagnostics and confidence/continuity tests. Model-based beat tracking can be revisited only after
the release-critical audio engine and routing work are stable.
