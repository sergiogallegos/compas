# Real-time audio programming notes: Bencina + Doumler

## Citation

- Ross Bencina, "Real-time audio programming 101: time waits for nothing"
  - Link: http://www.rossbencina.com/code/real-time-audio-programming-101-time-waits-for-nothing
  - Status: canonical source URL is known and referenced by Doumler, but direct download from this
    environment was refused. Re-fetch before citing detailed claims from the article.
- Timur Doumler, "Using locks in real-time audio processing, safely"
  - Link: https://timur.audio/using-locks-in-real-time-audio-processing-safely
  - Date: 2020-04-14
  - Local file: `docs/research/papers/doumler-locks-rt-audio.html` (ignored)
  - Status: downloaded, read, and summarized.
- Related talk to verify later: Timur Doumler, "C++ in the Audio Industry" / CppCon 2015 audio
  talk. Doumler's article links to `https://www.youtube.com/watch?v=boPEO2auJj4`, but the exact
  talk title/page still needs confirmation before detailed claims are added.

## Core ideas

- Audio callbacks run under hard deadlines measured in a few milliseconds. Missing the deadline
  becomes an audible glitch, not just a slow frame.
- The callback must avoid operations with unknown or scheduler-mediated latency: allocation,
  locks, blocking I/O, system calls, logging, and panic/unwind paths.
- `try_lock` on the audio thread is not enough for `std::mutex`-style locks if the unlock path can
  interact with the OS scheduler or wake another thread.
- Preferred communication patterns:
  - plain atomics for small parameter values;
  - single-producer/single-consumer queues for streams of commands/events;
  - immutable snapshots for larger graphs/data structures;
  - control-thread reclamation for retired state.
- If a lock-like fallback is unavoidable, the audio thread should only attempt a bounded
  non-blocking access and use a degraded fallback on failure. The waiting/backoff loop belongs on
  the non-audio thread, not the callback.
- Measurements should drive hardening work. Callback load, over-budget count, ring-full count,
  telemetry drops, and reclaim failures should be observable separately.

## Current compas behavior

- Good:
  - `compas-audio` owns `cpal::Stream` on a dedicated audio thread.
  - UI/control commands cross to the callback through an `rtrb` SPSC command ring.
  - Local decks use immutable `Arc<DeckBuffer>` data read by the callback.
  - Retired deck/sample buffers usually go through a reclaim ring and are drained by the control
    side.
  - DSP processors are preallocated and documented as `RT-SAFE`.
  - Callback load and xrun count are already published.
- Needs follow-up:
  - `Mixer::retire` currently ignores reclaim push failure; if the ring is full, the `Arc` can be
    dropped at the end of the function on the callback path.
  - `LoadSample` / `ClearSample` also ignore reclaim push failure.
  - The current xrun telemetry combines several possible causes; command-ring full, cue underrun,
    record overflow, telemetry drop, and reclaim-ring full should be split.
  - `build_stream` uses `std::time::Instant::now()` in the callback for load measurement. This may
    be acceptable enough for diagnostics, but it is still a timing/syscall-adjacent dependency that
    should be validated on Windows/macOS/Linux.
  - Architecture comments had stale PCM-ring wording; corrected in this pass.

## Useful changes

1. Add explicit counters:
   - callback over-budget;
   - command ring full;
   - reclaim ring full;
   - cue ring underrun/re-prime;
   - record ring overflow/drop;
   - telemetry publish/drop if a future ring is added.
2. Make reclaim failure non-dropping:
   - either increase reclaim capacity and assert/test it cannot fill under normal command bursts;
   - or keep a small fixed RT-side retired-buffer parking area drained by the control thread;
   - or make load/eject commands fail/defer when reclaim space is unavailable.
3. Add load/eject/reload tests around `Arc<DeckBuffer>` retirement and sampler slot replacement.
4. Add a static RT-audit checklist to code review: no allocation, locks, logging, I/O, blocking,
   panic, unbounded loops, or large drops in callback-reachable functions.
5. Treat the future modular deck graph as an immutable snapshot built off-thread and read-only on
   the callback.

## Risks / non-goals

- Do not add locks to the audio path to solve reclaim, routing, or graph mutation.
- Do not introduce a dynamic plugin-style graph until there is an immutable snapshot handoff.
- Do not hide callback failures behind one generic xrun count; split the counters first.
- Do not rewrite beat tracking in this RT pass. Beat tracking is offline/analysis unless live input
  is explicitly added.

## Tests or benchmarks required

- Unit test: repeated deck `LoadDeck`/`UnloadDeck` with a tiny reclaim ring records reclaim pressure
  and never drops large buffers on the callback path.
- Unit test: sampler load/clear replacement follows the same reclaim rule.
- Unit test: command-ring full is observable from `AudioEngine::send`.
- Bench/check: callback processing budget with worst-case enabled deck graph (4 decks, key-lock,
  FX, sampler, cue, record).
- Platform smoke: load telemetry timing does not cause measurable callback spikes on Windows.

## Decision

Adopt the guidance. compas already uses the correct broad architecture: SPSC commands, immutable
buffers, and off-thread decode/analysis. The next implementation should not start with sync math.
It should first strengthen RT observability and reclaim guarantees so later sync/routing/graph work
has safer foundations.
