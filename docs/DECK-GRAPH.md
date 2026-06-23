# compas deck processing graph

This is the target structure for one local-file deck. It is the contract for future work that adds
stems, ReplayGain, richer FX chains, mic/aux routing, and recording policies without threading more
ad hoc logic through `Mixer::next_frame`.

```text
source -> playhead/resampler -> keylock -> pregain/ReplayGain -> EQ/filter -> FX -> fader -> buses
```

## Goals

- Keep the audio callback allocation-free, lock-free, syscall-free, and bounded.
- Make each per-deck stage explicit enough to test in isolation.
- Make future graph swaps use the same off-thread reclaim rule as retired `Arc<DeckBuffer>` values.
- Keep routing decisions in the bus/routing layer, not hidden inside individual DSP stages.

## Stage Contract

### 1. Source

Input is an immutable decoded local source:

- today: one `Arc<DeckBuffer>` with interleaved stereo f32;
- future stems: one immutable snapshot containing multiple `Arc<DeckBuffer>` stems plus routing
  metadata.

The source object is installed by command handoff. Old source snapshots must be retired through the
control-thread reclaim path or bounded RT-side parking.

### 2. Playhead / Resampler

Owns transport position and source-rate to device-rate conversion:

- fractional playhead in source frames;
- cubic-Hermite interpolation for normal varispeed and scratch;
- loop, loop-roll/slip, seek, scratch, and sync playhead movement;
- clamps at source bounds without panicking.

This stage produces one stereo frame at device rate. It should not know about EQ, FX, faders, or
output buses.

### 3. Keylock

Owns time-stretch / master-tempo processing:

- engaged only when keylock is active and scratch is inactive;
- re-primes when playhead jumps, keylock changes, or stretch mode changes;
- all stretch buffers are allocated at construction/reset time, not in `process`.

The stage takes the playhead/resampler frame stream and returns one pitch-stable stereo frame.

### 4. Pregain / ReplayGain

Applies static and smoothed level normalization before tone/FX:

- ReplayGain or library loudness factor;
- trim/pregain smoother;
- future auto-gain policy.

This is pre-EQ/filter and pre-FX so downstream processors see predictable levels. User-facing
channel fader remains later in the graph.

### 5. EQ / Filter

Owns tonal shaping:

- three-band EQ per channel;
- DJ filter knob, implemented as LPF/HPF/off;
- coefficient changes are command-side updates to pre-existing processor state.

The current code runs filter then EQ. The target graph treats them as one tonal block; preserve
audible behavior until a dedicated migration test says otherwise.

### 6. FX

Owns ordered insert effects:

- current default chain: echo, reverb, flanger, bitcrusher;
- future chains can reorder/replace slots if construction happens off the callback;
- slot enable, mix, and parameter changes must remain bounded and RT-safe.

FX state that is replaced wholesale must be retired through the same no-drop path as deck buffers.

### 7. Fader

Owns the deck's post-FX level contribution:

- channel fader / deck gain;
- crossfader assignment is a routing decision applied after the deck frame exists;
- smooth every scalar control that can jump.

The target separates pregain from channel fader so library loudness, gain staging, FX drive, and
performance level can be reasoned about independently.

### 8. Buses

The mixer routes the final deck frame to explicit buses:

- master;
- cue/headphones;
- booth;
- record;
- future mic/aux/stem buses.

Bus taps should live under the output-routing model. A deck stage should not push to record/cue
rings directly.

## Ownership Model

The graph object should be split into:

- `DeckGraphPlan`: control-thread data describing source, stage configuration, effect order, and
  routing policy.
- `DeckGraphRuntime`: audio-thread processors and smoothers, preallocated before use.
- `DeckGraphSnapshot`: immutable or RT-owned state installed by command handoff.

Graph replacement must be pointer/snapshot swap only. Any old snapshot containing large buffers or
heap-backed processor state is retired through the reclaim ring and RT-side parking fallback.

## Current Implementation Mapping

The graph lives in `crates/compas-audio/src/mixer.rs`. Several stages are now extracted into
dedicated structs; the playhead/source read is the remaining inline block.

Extracted stage structs (each tested in isolation, behavior-preserving):

- **`KeylockStage`** (stage 3) — the key-lock toggle, the WSOLA mix stretcher, the per-stem
  stretchers, and the `engaged` re-prime flag. `begin_frame(scratching)` computes whether stretched
  reading is active and re-primes on the engage edge; `mark_jumped()` flags a play-head jump
  (seek / loop / scratch release / stem swap); `set_active()` drives the toggle.
- **`ToneStage`** (stage 5) — DJ filter → 3-band EQ, per channel. `process(l, r)`, `set_filter`,
  `set_eq`.
- **`DeckFxStage`** (stage 6) — `FxChain` already has a state/processor split and serves as this
  stage directly. (Wholesale FX-chain replacement still needs the no-drop retire path when a reorder
  command is added.)
- **`FaderStage`** (stage 7) — channel-gain smoother + ReplayGain factor. `advance()` ticks the
  smoother every frame (click-free unpause); `apply(l, r)` multiplies the post-FX frame by
  gain × replay gain. ReplayGain is still applied post-FX; the gain-staging split (moving it ahead of
  the tone block) is deferred — see migration step 4.

Source read + play-head advance (stages 1–2) are extracted as **methods** on `DeckPlayer`, not
separate structs:

- **`read_source_frame(engaged)`** (stages 1–2 read) — samples the source at the current play-head:
  sums the stems (each at its smoothed gain) when loaded, otherwise reads the mix buffer, through
  `KeylockStage` when engaged or direct cubic-Hermite interpolation otherwise.
- **`advance_playhead(max)`** (stage 2 transport) — moves the play-head one frame: scratch rate, or
  sync (PLL) / user tempo, plus the loop-roll shadow play-head and beat-loop wrap.

`next_frame` is now a readable pipeline: `fader.advance` → early-outs → `read_source_frame` →
`advance_playhead` → `tone` → `fx` → `fader.apply`.

These stay methods (sharing `DeckPlayer`'s transport fields) rather than a `PlayheadStage` struct on
purpose: `playhead` and the loop/sync/scratch fields are read/written ~100× across the audio-thread
**sync PLL**, telemetry publishing, and the seek/loop/scratch/beat-jump command handlers. The PLL in
particular needs simultaneous `&mut` access to *two* decks' play-heads, which the flat-field layout
supports cleanly; moving the play-head into a sub-struct would cascade through all those sites and
fight the borrow checker in RT-critical code, for no behavior gain. The methods give the
isolation/testability benefit (each is unit-tested) at near-zero risk.

Still inline:

- `Mixer::next_frame` applies crossfader assignment and routes to master/cue/booth/record taps.
- `OutputRouting` owns record, cue, and booth sinks.

## Migration Plan

1. Add small stage structs without changing audible behavior:
   - `DeckSourceStage` — ✅ extracted as the `read_source_frame` method (see note above)
   - `PlayheadStage` — ✅ extracted as the `advance_playhead` method (see note above)
   - `KeylockStage` — ✅ done
   - `ToneStage` — ✅ done
   - `DeckFxStage` — ✅ `FxChain` already serves as this stage
   - `DeckFaderStage` — ✅ done (`FaderStage`)
2. Move code from `DeckPlayer::next_frame` stage by stage, keeping existing tests green after each
   move.
3. Add stage-level tests before changing behavior:
   - playhead clamps and loops;
   - keylock re-prime on seek;
   - pregain before tone block;
   - FX bypass/order;
   - fader smoothing and crossfader assignment;
   - cue/booth/record bus independence.
4. Once structure is stable, move ReplayGain before the tone block if listening tests and regression
   tests confirm the gain staging is correct.
5. Use the snapshot/reclaim path for any graph swap that owns buffers or heap-backed processor
   state.

## Non-Goals For The First Refactor

- No dynamic heap allocation in the audio callback.
- No scripting inside the callback.
- No stem graph until the mono/stereo source graph is split cleanly.
- No change to streaming decks; streaming remains control-only until compas owns decoded PCM.
