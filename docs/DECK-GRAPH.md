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

Today, the graph is mostly inline inside `crates/compas-audio/src/mixer.rs`:

- `DeckPlayer::next_frame` combines playhead, keylock, filter/EQ, FX, ReplayGain, and deck gain.
- `Mixer::next_frame` applies crossfader assignment and routes to master/cue/booth/record taps.
- `OutputRouting` owns record, cue, and booth sinks.
- `FxChain` already has a state/processor split and is the closest current model for a modular stage.

## Migration Plan

1. Add small stage structs without changing audible behavior:
   - `DeckSourceStage`
   - `PlayheadStage`
   - `KeylockStage`
   - `ToneStage`
   - `DeckFxStage`
   - `DeckFaderStage`
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
