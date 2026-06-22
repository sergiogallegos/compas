# Lock-free state handoff design note

This note turns the real-time audio research into a concrete compas design rule: the audio callback
must read stable state and communicate through bounded, observable handoff paths. No shared mutable
graph, no audio-thread locks, no best-effort silent drops.

## Current handoff paths

| Path | Direction | Mechanism | Current status |
|---|---|---|---|
| Control commands | control/UI -> audio callback | `rtrb::Producer<AudioCommand>` / `Consumer<AudioCommand>` | Good foundation. `AudioEngine::send` returns an error when full. Needs telemetry counter. |
| Deck audio | worker/control -> audio callback | immutable `Arc<DeckBuffer>` installed by `AudioCommand::LoadDeck` | Good foundation. The callback reads by fractional play-head. |
| Buffer reclamation | audio callback -> control side | `rtrb::Producer<Arc<DeckBuffer>>` / `Consumer<Arc<DeckBuffer>>` | Needs hardening. Push failure can still lead to callback-side drop. |
| Telemetry | audio callback -> control/UI | relaxed atomics in `DeckTelemetry` | Good foundation for scalar state. Needs split diagnostic counters. |
| Recording | audio callback -> writer thread | `rtrb::Producer<f32>` | Works. Needs overflow/drop counter. |
| Cue/headphones | audio callback -> cue stream thread | `rtrb::Producer<f32>` | Works. Needs underrun/re-prime counter from cue side. |

## Design rules

### 1. Commands are bounded and observable

Commands are allowed to fail when the ring is full. That failure must be visible. For compas:

- `AudioEngine::send` should increment a command-ring-full counter when `push` fails.
- UI/controller layers should coalesce high-rate controls before sending where practical.
- Audio command handling must remain O(1) per command and bounded by ring capacity.

### 2. Audio data is immutable on the callback

Decks and samples use `Arc<DeckBuffer>`. Once a buffer is visible to the callback, its audio data is
immutable.

Implications:

- Do not mutate `DeckBuffer` in place.
- Stem buffers should follow the same rule: workers produce immutable buffers, then install them by
  command handoff.
- The future per-deck graph must not hold references to mutable UI/control data.

### 3. Retired state needs a no-drop guarantee

Replacing deck buffers, sample buffers, stem buffers, or future graph snapshots can retire large
state. The callback must not drop the final reference.

Current implementation:

- `Mixer::retire` pushes retired deck/sample buffers into the control-thread reclaim ring.
- If that ring is full, the mixer stores the old `Arc<DeckBuffer>` in a fixed-size RT-side parking
  area and retries after command draining.
- `DeckTelemetry::reclaim_ring_full` counts those pressure events and the UI title-bar tooltip
  exposes them separately from audio drops.
- If parking is pathologically exhausted, the buffer is intentionally leaked rather than dropped on
  the callback path.

Implemented tests:

- deck buffer retirement while the reclaim ring is full;
- sampler replacement while the reclaim ring is full;
- parked buffers flush after the reclaim consumer drains.

### 4. Telemetry is scalar and split by failure mode

One xrun count is not enough for release hardening. Keep scalar atomics, but split causes:

- callback over-budget;
- command-ring full;
- reclaim-ring full;
- record-ring overflow/drop;
- cue-ring underrun/re-prime;
- stream error callback count.

### 5. Future graph state is immutable snapshot state

The target deck graph is:

```text
source -> playhead/resampler -> keylock -> pregain/ReplayGain -> EQ/filter -> FX -> fader -> buses
```

Implementation rule:

- control thread builds or mutates graph state off-thread;
- audio thread reads a stable snapshot;
- processors allocate during construction/reset, not while processing;
- graph swaps retire old snapshots through the same reclaim model as `DeckBuffer`;
- graph command application is O(1), e.g. swap an index/snapshot pointer, change a scalar target, or
  toggle a preallocated processor.

## Concrete next code slice

1. Extend `DeckTelemetry` or a sibling `EngineDiagnostics` with split counters.
2. Increment command-ring-full in `AudioEngine::send`.
3. Done: increment reclaim-ring-full in `Mixer::retire` and sampler replacement.
4. Done: prevent callback-side large drops when reclaim is full for current `DeckBuffer` retire
   paths.
5. Done: add tests for deck and sampler replacement under reclaim pressure.
6. Next: route future large graph/stem snapshots through the same retire model.

## Non-goals for this slice

- No booth output yet.
- No modular graph implementation yet.
- No sync algorithm changes yet.
- No beat-tracking changes yet.

The purpose of this slice is to make later changes measurable and safer.
