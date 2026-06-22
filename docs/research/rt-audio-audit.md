# compas real-time audio audit checklist

Use this checklist before changing `crates/compas-audio`, `crates/compas-dsp::rt`, routing,
recording, cue output, controller feedback, or the future modular deck graph.

## Callback boundary

Callback-reachable code starts at:

- `crates/compas-audio/src/engine.rs` `build_stream` output callback
- `crates/compas-audio/src/mixer.rs` `Mixer::drain_commands`
- `crates/compas-audio/src/mixer.rs` `Mixer::next_frame`
- any function marked `RT-SAFE`
- cue output callback in `crates/compas-audio/src/cue.rs`

## Hard no

The callback path must not:

- allocate heap memory;
- lock a mutex or rwlock;
- wait on a condvar, channel, thread, process, async runtime, or device;
- do file, network, database, or OS-keychain I/O;
- log;
- panic, unwrap, expect, or intentionally unwind;
- call into Tauri, WebView, DB, Spotify, HID enumeration, MIDI enumeration, or filesystem APIs;
- run unbounded loops over user/data-dependent sizes;
- drop large or ownership-heavy state such as `Arc<DeckBuffer>` if that may be the last reference.

## Allowed patterns

- SPSC rings (`rtrb`) with bounded capacity and explicit failure handling.
- Relaxed atomics for telemetry and small numeric state.
- Preallocated DSP processors with fixed-size internal buffers.
- Immutable `Arc<DeckBuffer>` reads.
- Control-thread built state snapshots that the callback only reads.
- Failure-to-silence or hold-last-value fallbacks when a noncritical RT resource is unavailable.

## Required review questions

1. Can this code allocate through `Vec`, `String`, `format!`, `Box`, `Arc::new`, collection growth,
   trait-object boxing, or hidden library behavior?
2. Can this code drop the last reference to a large buffer or processor state?
3. Can this code lock, unlock, wake, notify, yield, sleep, or touch the scheduler?
4. Can this code call logging/tracing, even only on an error branch?
5. Is every loop bounded by a compile-time constant or small fixed capacity?
6. Is every ring-full/empty case accounted for with telemetry?
7. If this is a command, can a burst of UI/controller messages overflow the command ring?
8. If this retires state, what happens when the reclaim path is full?
9. If this touches devices, can a hot-plug/removal failure block or panic?
10. Is the behavior covered by a deterministic test or benchmark?

## Current audit findings

| Area | Status | Notes |
|---|---|---|
| Command handoff | Good foundation | `AudioEngine::send` uses an SPSC command ring and returns an error if full. Need a dedicated counter/UI surface for command-ring full. |
| Deck buffers | Good foundation | Local audio is immutable `Arc<DeckBuffer>` and read via fractional play-head. |
| Buffer reclaim | Needs hardening | `Mixer::retire` and sampler replacement push into a reclaim ring but ignore push failure. If full, a buffer can be dropped on the callback path. |
| Callback telemetry | Partial | RT load and xrun count exist. Need split counters for command full, reclaim full, cue underrun, record overflow, and stream errors. |
| Cue output | Partial | Dedicated stream and ring exist. Need explicit cue underrun/re-prime counter. |
| Recording | Partial | Writer thread and ring exist. Need explicit record-ring overflow/drop counter. |
| Modular graph | Planned | Future graph must use immutable snapshots and off-thread reclamation. |
| Timing measurement | Needs validation | `Instant::now()` in callback supports RT-load telemetry; verify platform cost and consider build/runtime switch if needed. |

## First implementation slice

Before sync edge-case tests, add observability and reclaim safety:

1. Add telemetry fields/counters for command-ring full, reclaim-ring full, cue underrun, record
   overflow, and stream error.
2. Change reclaim failure from silent best-effort to an explicit bounded fallback with a counter.
3. Add tests for deck load/unload and sampler replace under reclaim pressure.
4. Keep public UI minimal at first: title/status can expose the split counters later.
