# Live / online beat-tracking — design note

Adoption-plan **slice 5**. The gate (`docs/research/beat-tracking-adoption-plan.md`) requires this
design note *before any online-tracker code enters the engine*, because live tracking trades future
context for causality and must not touch offline local-file analysis or the audio callback. This
note defines the goal, the real-time boundary, the algorithm direction, the routing/sync contract,
and a phased implementation + test plan. It does **not** change any code.

Sources: OBTAIN (Mottaghi 2017, `summaries/obtain-online-beat-tracking.md`) as the verified causal
reference; BeatRoot/Dixon trail (`summaries/beat-tracking-literature.md`) for keeping tempo
induction and phase tracking conceptually separate and carrying multiple hypotheses. The requested
"zero-latency 2024" citation remains unverified — do not implement under that name.

## 1. Goal and non-goals

**Goal.** Continuously estimate the tempo and beat phase of a *live* audio input — the mic/aux
capture added in `compas-audio::input`, and later external gear or a streaming control-only feed —
causally (no look-ahead), and expose it as a "live beat clock" the rest of the app can read or sync
to (e.g. a turntablist/band drummer that the loaded decks beat-match to, or a tempo readout for an
incoming line signal).

**Non-goals.**

- **Not** a replacement for offline local-file analysis. Loaded tracks already have RAM and full
  future context; they keep using `compas-dsp::analysis`. This path is *only* for sources that have
  no future (`mic`, `aux`, external gear, streaming control-only timing).
- **Not** in the cpal callback. Even a causal tracker buffers, and our DSP allocates; it runs on a
  separate analysis thread (see §3).
- **Not** a new dependency. Stay pure-Rust/unsafe-free like the rest of `compas-dsp`; reuse the
  existing spectral-flux onset envelope.

## 2. Why this is different from offline analysis

`estimate_beatgrid` autocorrelates the *whole* envelope and combs *all* phases — it sees the future.
A live tracker only ever has the past. Consequences that shape the design:

- Tempo must be induced over a **sliding window** (e.g. the last 6–12 s), updated incrementally.
- Phase must be a **forward predictor** (a beat-period oscillator / PLL corrected by onsets), not an
  argmax over the whole signal.
- It must **converge from a cold start** and **re-lock** after tempo changes, dropouts, or false
  onset bursts — and report low confidence while unlocked rather than emit a wrong grid.

## 3. Real-time boundary and threading

```text
  input device (cpal, RT)        analysis thread (NOT RT)         control/UI
  ─────────────────────          ────────────────────────        ──────────
  capture callback ──┬─ aux ring ──▶ Mixer.next_frame()  (master sum, already shipped)
                     └─ analysis ring ──▶ LiveTracker.push(chunk)
                                              │  onset → tempo(window) → phase PLL
                                              ▼
                                       LiveBeatClock (atomics): bpm, phase, confidence, locked
                                              │
                                              ▼  (telemetry, like DeckTelemetry)
                                        UI readout  +  optional virtual sync leader (§5)
```

- The cpal **input callback fans out** to two SPSC rings: the existing aux→mixer ring, and a new
  analysis ring. The capture callback stays trivial (two `push`es); on analysis-ring overflow it
  drops (the tracker tolerates gaps). This keeps the analysis off both the input and output
  callbacks.
- A dedicated **analysis thread** owns the `LiveTracker`, drains the analysis ring in
  hop-sized chunks, and runs onset→tempo→phase. Per-chunk cost must be **bounded and
  allocation-free after warmup** (preallocate the sliding-window buffers). It is *not* RT-critical
  (a late update just delays a beat estimate) but should be cheap (target < 1 ms per ~12 ms hop).
- Output is published to a lock-free `LiveBeatClock` (atomics, same pattern as `DeckTelemetry` /
  `MonitorLatency`): `bpm`, `beat_phase` (0..1, with a reference timestamp), `confidence`, `locked`.

## 4. Algorithm direction (OBTAIN-style, causal)

Keep tempo induction and phase tracking separate (BeatRoot principle):

1. **Onset strength** — reuse `spectral_flux_envelope` per hop (already in `analysis.rs`); maintain
   a ring of the last N envelope samples (sliding window).
2. **Tempo (windowed)** — autocorrelation / comb over the window each update, reusing the existing
   `MIN_BPM..MAX_BPM` range, octave handling (`select_tempo`), and density weighting. Smooth the
   tempo estimate over time; carry half/double alternatives rather than committing instantly.
3. **Phase (forward PLL)** — a beat-period oscillator predicts the next beat; each detected onset
   near a predicted beat nudges the oscillator's phase (bounded correction, like the deck sync PLL's
   `SYNC_PHASE_GAIN`/`SYNC_MAX_BEND`). Cumulative beat-strength (OBTAIN) scores predicted vs actual
   onsets to gate corrections and drive confidence.
4. **Confidence / lock** — reuse the calibrated-confidence ideas (periodic strength × octave ×
   rival). Below a threshold → `locked = false`, and downstream sync must refuse to follow (mirrors
   the existing "follower should not snap to a bad leader grid" rule).

This is deliberately the *same DSP vocabulary* as the offline path, run incrementally — not a new
model. It reuses tested building blocks and keeps the no-GPL, pure-Rust constraint.

## 5. Routing / sync contract

The live clock plugs into the **existing deck sync PLL** as a *virtual leader*. Today `update_sync`
rate-matches a follower deck to a real leader deck via `sync_master: Option<usize>` and the
leader's `(playhead, beat_offset, beat_interval, advance)`. The contract:

- Introduce a **virtual leader source** the follower's PLL can target instead of a deck — e.g.
  `sync_master: SyncSource` where `SyncSource` is `Deck(usize)` or `Live`. `update_sync` reads the
  `LiveBeatClock`'s beat rate and phase exactly where it reads a leader deck's today (same
  rate-match + bounded phase bend, same Full vs TempoOnly modes).
- **Only follow when `locked`.** An unlocked or low-confidence live clock makes the follower hold
  its last tempo (no snap), identical to the bad-leader guard.
- This also resolves the long-deferred **"sync internal-clock virtual leader"** item (STATUS) — the
  same `SyncSource::Live` mechanism can later host an internal metronome/clock, not just the mic.
- **Clock domains:** the input device, the output device, and the analysis thread are independent
  clocks. The beat clock is published with a reference timestamp; consumers extrapolate. The mic→
  master path already rides drift via the aux prime buffer; the tracker only needs tempo/phase, so
  small drift is absorbed by the PLL.
- **Latency honesty:** report the input + analysis latency so the UI can show "live BPM (≈X ms
  behind)". A live follower is inherently behind the source; surface it, don't hide it
  (capability-honest UI rule).

## 6. Phased implementation plan (each its own reversible slice)

1. **`LiveTracker` core in `compas-dsp`** — pure, offline-testable struct: `push(&[f32]) ->
   Option<LiveEstimate>`, sliding-window onset+tempo+phase, allocation-free after `new`. No engine
   wiring. Tested entirely with a streaming-chunk harness (§7).
2. **Capture fan-out + analysis thread** — `open_aux_input` gains an optional second producer;
   `compas-audio` spawns the analysis thread and publishes a `LiveBeatClock` (atomics). IPC +
   telemetry to surface live BPM/confidence/locked in the UI. Still no deck sync.
3. **Virtual-leader sync** — `SyncSource::Live` in the mixer PLL + IPC to point a deck at the live
   clock; UI control ("SYNC → MIC"). Guarded by `locked`.
4. **Polish** — latency readout, external-gear/streaming-control-only sources reusing the same
   clock, internal-metronome virtual leader.

Ship 1 first and stop for review — it has zero engine surface and proves the algorithm on
synthetic streams before any real-time plumbing.

## 7. Test / benchmark plan

A streaming harness feeds the tracker fixed-size chunks (no look-ahead) and asserts convergence:

- **Cold start** — locks to a steady tempo within a bounded number of beats; `locked=false` until
  then.
- **Tempo change** — re-locks after a step (e.g. 120→128) and after a ramp, within a bound.
- **Missed onsets / dropouts** — holds tempo through gaps; doesn't unlock prematurely.
- **False-onset bursts** — noise/extra hits don't yank phase (bounded correction).
- **No-lookahead invariant** — feeding the same signal one chunk at a time vs all-at-once must give
  the *causal* result (a test that the tracker never peeks ahead).
- **Bounded cost** — `cargo bench` proves per-chunk time is bounded and independent of total stream
  length (the offline `estimate_tempo` benches stay the regression baseline for the shared DSP).
- **Confidence** — ambiguous/half-double live input reads lower confidence than a clean click, same
  as the offline matrix.

`cargo test -p compas-dsp --locked` + `cargo bench -p compas-dsp` per the standing gate; no offline
fixture may regress (the `LiveTracker` reuses but does not modify `estimate_*`).

## 8. Gate checklist (for the eventual implementation commits)

- **Separate design note first:** this document. ✅ (precondition met)
- **No local-file beatgrid change without offline benchmarks:** `LiveTracker` is additive; offline
  `estimate_*` untouched; matrix + benches stay green. ▶ enforced per slice.
- **Explicit routing/sync contract** for mic/aux/external gear/streaming control-only timing: §5. ✅
- **RT boundary:** analysis off the audio thread; bounded, allocation-free per-chunk. §3. ✅
- **Rollback:** each slice (core → fan-out → virtual-leader) reverts independently. §6. ✅

## 9. Open questions / risks

- **Convergence latency vs stability** — tighter PLL gain locks faster but jitters; needs tuning
  against the streaming harness (mirrors `SYNC_PHASE_GAIN` tuning).
- **Octave on live input** — without future context the octave can flip mid-stream; the dance-tempo
  prior helps but live half/double is harder than offline. Carry alternatives, expose confidence.
- **Second input stream vs fan-out** — fan-out from one capture callback is preferred (one device
  open, one clock); confirm rtrb multi-producer isn't needed (it's one callback pushing to two
  SPSC rings, which is fine — single producer per ring).
- **External gear / streaming** — external gear may arrive as audio (use this tracker) or as MIDI
  clock (a different, exact path — not this note). Streaming is control-only (no PCM), so it has no
  live audio to track; its "timing" would come from provider metadata, out of scope here.
