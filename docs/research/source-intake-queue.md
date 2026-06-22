# Research source intake queue

Use this file as the working queue before changing audio, sync, beat tracking, or the deck graph.
The local downloads live in `docs/research/papers/`, which is intentionally ignored by git.

## Read order

1. `AGENTS.md`, `ARCHITECTURE.md`, and `docs/DECK-GRAPH.md`.
2. `docs/research/rt-audio-audit.md` and `docs/research/lock-free-state-handoff.md`.
3. `docs/research/summaries/rt-audio-bencina-doumler.md`.
4. `docs/research/summaries/beat-tracking-literature.md`.
5. `docs/research/summaries/obtain-online-beat-tracking.md`.
6. `docs/research/summaries/beat-this-2024.md`.
7. `docs/research/beat-tracking-adoption-plan.md`.

Do not start an algorithm rewrite until the relevant source summary names one target behavior,
the test that will fail/pass, and the benchmark or telemetry check used to keep the change honest.

## Source status

| Source | Local file | Status | Immediate compas action |
|---|---|---|---|
| Ross Bencina, "Real-time audio programming 101: time waits for nothing" | Pending; site refused connection from this environment | Canonical URL known and cross-referenced by Doumler | Keep as required reading; do not quote detailed claims until fetched locally or manually verified. |
| Timur Doumler, "Using locks in real-time audio processing, safely" | `papers/doumler-locks-rt-audio.html` | Downloaded and summarized | Continue the immutable graph + SPSC + no-drop reclaim direction already started. |
| Timur Doumler, "C++ in the Audio Industry" / CppCon audio talk | Pending exact-title verification | Linked indirectly from Doumler article as a CppCon 2015 talk | Find exact talk page before adding detailed claims. |
| Dixon / BeatRoot beat tracking trail | Pending primary PDF/title verification | Secondary trail only | Use only as a design hint: multi-hypothesis tempo/phase candidates and half/double ambiguity tests. |
| Laroche (2003), "Efficient Tempo and Beat Tracking in Audio Recordings" | Pending citation verification | Not verified | No implementation under this citation until DOI/venue/title is confirmed. |
| Mierer et al. (2024), "A real-time beat tracking system with zero latency" | Pending citation verification | Not found by exact-title and author search | Treat as unresolved; OBTAIN is the verified online fallback for now. |
| Mottaghi et al. (2017), "OBTAIN: Real-Time Beat Tracking in Audio Signals" | `papers/mottaghi-obtain-2017.pdf` | Downloaded, extracted, summarized | Useful for future live-input/online tracking design, not a replacement for offline deck analysis yet. |
| Foscarin, Schlueter, Widmer (2024), "Beat this! Accurate beat tracking without DBN postprocessing" | `papers/foscarin-beat-this-2024.pdf` | Downloaded, extracted, summarized | Useful as a modern evaluation warning: accuracy and continuity can trade off; keep confidence/continuity tests. |

## Implementation gates

- **Real-time audio:** every patch must answer the checklist in `rt-audio-audit.md`. If a callback
  path can allocate, block, log, panic, run unbounded, or drop large state, stop and redesign.
- **Deck graph:** graph state must be control-thread built and callback-read-only. Retired graph
  snapshots must use the same no-drop retirement model as deck/sample buffers.
- **Beat tracking:** implement diagnostics and confidence calibration before changing tempo
  selection. Online/zero-latency tracking belongs behind a separate live-input design.
- **Papers:** commit only citations and summaries. Keep PDFs and extracted text ignored unless a
  source explicitly permits redistribution and the maintainer decides to vendor it.
