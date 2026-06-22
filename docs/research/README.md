# compas research intake

This folder is the working wiki for papers, talks, and implementation notes that may improve
compas. The goal is not to collect PDFs. The goal is to turn credible sources into small,
reviewable engineering decisions.

## Research workflow

1. **Read the local architecture first.**
   - `AGENTS.md`
   - `ARCHITECTURE.md` sections 4, 5, 5a, and 8
   - `ROADMAP.md` "Reliability / Pro-Audio Hardening Backlog"
   - Relevant crate code before proposing an implementation:
     - `crates/compas-audio` for callback/routing/device work
     - `crates/compas-dsp` for beat tracking, sync, and DSP
     - `crates/compas-core` for shared control/mapping types
2. **Read real-time audio sources.**
   Focus: hard callback deadlines, no blocking, no allocation, no scheduler interaction, immutable
   graph/state handoff, and measurement.
3. **Read lock-free/state-handoff sources.**
   Focus: SPSC queues, atomics, immutable snapshots, reclaim paths, and how to avoid dropping large
   objects on the audio thread.
4. **Read beat-tracking papers.**
   Focus: onset strength, tempo induction, beat hypothesis tracking, half/double tempo handling,
   online/zero-latency limits, and evaluation metrics.
5. **Write a summary before coding.**
   Each source needs a short note: what it says, what compas already does, what we should change,
   risks, and tests.
6. **Implement one small slice.**
   Prefer a benchmark/test or telemetry improvement first, then engine changes. Do not rewrite the
   beat tracker or routing graph in one large patch.

See `source-intake-queue.md` for the current verified/downloaded source status and the order to
read these notes before changing code.

## Download / citation policy

- Do not commit copyrighted PDFs unless the source explicitly permits redistribution.
- It is fine to keep local PDFs in an ignored personal folder; commit only links, citations, and
  summaries.
- If a paper has code, dataset, or model artifacts, record the license before using it.
- Every implementation inspired by a paper should cite the source in the summary, not necessarily
  in production code.

## Priority reading list

| Priority | Source | Status | Why it matters for compas | Candidate output |
|---|---|---|---|---|
| 1 | Ross Bencina, "Real-time audio programming 101: time waits for nothing" | Initial summary/audit added; direct fetch still pending | Canonical callback-deadline rules: no blocking, allocation, I/O, logging, or scheduler interaction on the audio thread. | RT-safety audit checklist; tests for no-drop/reclaim paths; stricter review rules. |
| 2 | Timur Doumler, "C++ in the Audio Industry" + "Using locks in real-time audio processing, safely" | Article verified; initial summary/audit added; exact talk link to confirm | Practical state-sharing patterns for audio apps: atomics, SPSC queues, immutable graph snapshots, and why mutex/try-lock patterns are risky. | Immutable deck-graph snapshot design; control-thread rebuild + RT-side read-only graph; lock-free handoff notes. |
| 3 | Simon Dixon, "A beat tracking system for audio signals" (2000) | Exact title not verified; BeatRoot trail documented | Classic multi-hypothesis beat tracking and tempo/beat-agent ideas. | Sync/beatgrid edge-case tests; half/double-tempo candidate tracking; confidence score for beatgrid. |
| 4 | Jean Laroche, "Efficient Tempo and Beat Tracking in Audio Recordings" (2003) | Citation still unverified | Efficient onset/tempo tracking methods useful for offline analysis and maybe low-latency online tracking. | Compare current spectral-flux/autocorrelation with Laroche-style tempo tracking; benchmark on synthetic + real files. |
| 5 | Mierer et al. (2024), "A real-time beat tracking system with zero latency" | **Citation not verified; possible misspelling/title mismatch** | If found, this is the most relevant source for online beat tracking without lookahead. | Decide whether zero-latency online tracking belongs in compas sync or only future live-input/mic work. |
| 6 | Mottaghi et al., "OBTAIN: Real-Time Beat Tracking in Audio Signals" (2017) | Downloaded and summarized | Online beat tracking with onset strength, tempo estimation, cumulative beat strength, and peak selection. | Fallback if the 2024 zero-latency citation cannot be found; evaluate for online/live-input roadmap. |
| 7 | Foscarin, Schlueter, Widmer, "Beat this! Accurate beat tracking without DBN postprocessing" (2024) | Downloaded and summarized | Modern beat/downbeat evaluation lesson: accuracy and continuity can trade off; fixed tempo/meter constraints fail on harder music. | Add confidence/continuity tests before any estimator rewrite; do not adopt a model path yet. |

Known links:

- Ross Bencina: `https://www.rossbencina.com/code/real-time-audio-programming-101-time-waits-for-nothing`
- Timur Doumler article: `https://timur.audio/using-locks-in-real-time-audio-processing-safely`
- OBTAIN arXiv: `https://arxiv.org/abs/1704.02216`
- Beat this arXiv: `https://arxiv.org/abs/2407.21658`

## Summary template

Create one summary per source under `docs/research/summaries/`:

```md
# Source title

## Citation
- Authors:
- Year:
- Link:
- License / redistribution:

## Core ideas
- ...

## Current compas behavior
- ...

## Useful changes
- ...

## Risks / non-goals
- ...

## Tests or benchmarks required
- ...

## Decision
- Adopt / reject / revisit after ...
```

## Initial implementation candidates

1. **RT safety audit from Bencina/Doumler**
   - Verify no callback path allocates, locks, logs, blocks, or drops large state.
   - Add focused tests around deck load/eject/reload and graph-state retirement.
   - Extend telemetry for command-ring full, cue underrun, record overflow, and callback over-budget.
2. **Immutable per-deck graph handoff**
   - Control thread builds graph state.
   - Audio thread reads stable preallocated processors and `Arc<DeckBuffer>` references.
   - Retired buffers/processors go through a reclaim path.
3. **Beat tracking research harness**
   - Build a small benchmark/test corpus: synthetic clicks, swung drums, tempo ramps, silence,
     half/double tempo traps, and real music snippets kept out of git if copyrighted.
   - Compare current analysis against Dixon/Laroche/OBTAIN-inspired variants before replacing code.
4. **Beat tracking adoption gate**
   - Use `docs/research/beat-tracking-adoption-plan.md` before changing the estimator.
   - Implement one target behavior at a time with tests, benchmark cost, UI contract, and rollback
     path documented.
