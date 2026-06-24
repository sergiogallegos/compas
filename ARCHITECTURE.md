# compas — Architecture

> Living document. Update it in the same PR as any change to module layout, the threading
> model, the source abstraction, or the IPC surface.

compas is a cross-platform real-time DJ application. **Windows is the primary launch target;
macOS is first-class from day one** (no Windows-only dependencies). Linux is best-effort.
compas is **open-source (MIT)** and ships as a distributable binary, which shapes the streaming
design — a distributable can't embed a client secret, so streaming auth is public-client PKCE (§6).

---

## 1. The one idea that drives everything: two output paths

Streaming services give third parties **playback control, not decoded audio**. So compas has
two physically separate audio paths that **cannot be summed in our software buffer**:

```
   ┌─────────────────────────── compas (Rust core) ───────────────────────────┐
   │  LocalFileSource → decode (symphonia) → Arc<DeckBuffer> → DSP graph → Mixer ──┐   │
   └──────────────────────────────────────────────────────────────────────│───┘
                                                                            ▼
                                                                cpal output (WASAPI/CoreAudio)
                                                                            ▲  (acoustic / OS mix)
   ┌────────── webview (Spotify/Apple/SoundCloud SDK in the Tauri WebView) ─┘
   │  StreamingSource = control only (play/pause/seek/volume) → service audio → OS audio device
```

Consequences we **design around honestly** rather than paper over:

- A **local deck** gets the full DSP chain: gain, 3-band EQ, filter, time-stretch, beatgrid,
  sync, cue/loop, scratch, FX.
- A **streaming deck** gets transport control only. "Crossfading" from a local deck to a Spotify
  deck is done by inversely ramping our master gain **and** the SDK's volume; the two blend at
  the OS mixer, with unavoidable latency/clock-drift mismatch and **no sample-accurate sync**.
- The UI must **disable and explain** controls a streaming deck cannot perform (§7). We never
  render a fake EQ on audio we don't possess.

This is encoded in the type system (§3): only local sources implement `PcmSource`, so a streaming
deck *cannot* be routed into the DSP graph even by mistake.

## 2. Crate / module layout

A Cargo workspace. The pure-Rust engine crates are the core differentiator and are kept free of
Tauri, UI, and I/O concerns so they stay testable and portable.

```
compas/                      product family on a shared Rust core (see docs/ARCHITECTURE-PRODUCTS.md)
  Cargo.toml                 workspace; default-members = the shared-core + DJ engine crates
  crates/                    shared core (product-agnostic) + the DJ engine
    compas-core/             domain types: TrackMetadata, SourceCapabilities, DeckId, errors, control bus
    compas-dsp/              DSP: rt (real-time-safe biquads/EQ/crossfade/stretch) + analysis (offline BPM/key)
    compas-sources/          AudioSource abstraction: LocalFileSource (symphonia), decode/import
    compas-script/           sandboxed JS controller-scripting runtime (QuickJS)
    compas-audio/            Compás DJ engine: cpal output, lock-free rings, command protocol, Mixer
  apps/
    compas-dj/src-tauri/     Compás DJ Tauri 2 app (binary `compas-dj` + lib `compas_lib`); IPC commands
    compas-dj/frontend/      Compás DJ UI: React 19 + Vite + TS; WebGL waveforms; performance UI
  scripts/                   tooling (version check, test-audio, hooks)
  docs/                      design notes; product/architecture plans
```

`default-members` excludes `src-tauri` so `cargo check`/`test`/`clippy` run the engine crates
without needing WebView2/WebKitGTK or a built frontend. The app crate is built via the Tauri CLI
(which runs the frontend build first).

Dependency direction (no cycles):

```
compas-core ◀── compas-dsp
     ▲   ▲          ▲
     │   └── compas-audio ──┘
     └────── compas-sources
                  ▲
              src-tauri ──▶ (all of the above) ──▶ frontend (via IPC)
```

## 3. The source abstraction

We **revised** the proposed single-trait design. A single `AudioSource` with a
`fn capabilities() -> SourceCapabilities` static method is awkward for heterogeneous decks
(you need `&self` for instance capabilities and trait objects to hold mixed deck types), and —
more importantly — a single trait that "exposes sample buffers" invites runtime failures when you
call a sample method on a streaming source. Instead (`compas-sources/src/lib.rs`):

```rust
pub trait AudioSource {                 // every source: metadata + capability profile
    fn metadata(&self) -> &TrackMetadata;
    fn capabilities(&self) -> SourceCapabilities;
}

pub trait PcmSource: AudioSource {      // ONLY local files implement this
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
    fn next_chunk(&mut self) -> Result<Option<Vec<f32>>>;  // interleaved stereo, decoder thread
}
```

- `LocalFileSource: PcmSource` — decodes via symphonia; full DSP.
- `StreamingSource: AudioSource` (but **not** `PcmSource`) — metadata + `PLAYBACK_ONLY` caps; the
  real transport is driven from the frontend SDK via IPC. The engine binds the DSP graph against
  `PcmSource`, so streaming audio is *unrepresentable* in the sample path — a compile-time guarantee.

`SourceCapabilities { full_dsp, provides_pcm, can_seek, can_vary_tempo }` carries the invariant
`full_dsp ⇒ provides_pcm`. The UI reads it to gate controls.

## 4. Threading model

Three thread classes; the audio callback is sacred.

```
 control thread(s)              decoder thread(s)            audio callback thread (cpal, RT)
 ─────────────────              ─────────────────            ────────────────────────────────
 Tauri commands                LocalFileSource::decode_full   Mixer::drain_commands()  (lock-free)
   │  EngineMsg (mpsc)            │  Arc<DeckBuffer>            Mixer::next_frame() per frame
   ▼                              ▼                              │ read via fractional play-head
 compas-audio::AudioEngine ── AudioCommand (rtrb SPSC) ──▶ ────┘ apply deck graph + buses
   owns cpal::Stream
```

- **Audio callback (RT):** allocation-free, lock-free, syscall-free, no logging, bounded time,
  no panics. It only (a) drains the command ring and (b) reads immutable `Arc<DeckBuffer>` data
  through per-deck play-heads and mixes. Underruns increment a counter and emit silence; they
  never block.
- **`cpal::Stream` is `!Send` on some platforms**, so it is owned by a dedicated audio thread,
  **not** Tauri's shared state. Tauri commands send coarse `EngineMsg`s over a `std::sync::mpsc`
  to that thread, which forwards them as lock-free `AudioCommand`s.
- **Decoder/analysis workers** decode full local tracks into `Arc<DeckBuffer>` and run offline
  analysis/stem work. They may allocate, block, and touch disk; the callback never does.
- **Lock-free primitives:** `rtrb` SPSC rings for commands and RT-to-control telemetry/reclaim
  paths. One producer, one consumer, wait-free.
- **Buffer lifetime rule:** replacing/ejecting a deck must not drop the old `Arc<DeckBuffer>` on
  the audio callback. Retired buffers are first pushed to a control-thread reclaim ring; if that
  ring is full, the mixer parks them in a fixed-size RT-side holding area and retries after command
  draining. Pathological parking overflow intentionally leaks rather than freeing a large buffer on
  the callback path. Tests force deck and sampler replacement while the reclaim ring is full.

## 5. Audio data flow (a local deck, end to end)

1. **Load:** `LocalFileSource::open(path)` probes/decodes header (symphonia), fills
   `TrackMetadata` (duration, etc.). A worker decodes the full track, then the control thread
   installs the resulting `Arc<DeckBuffer>` on the target deck via command handoff.
2. **Analyze (offline, worker):** `compas-dsp::analysis` computes BPM/beatgrid and key from the
   decoded PCM, writes them back into `TrackMetadata` and the UI.
3. **Decode loop (worker):** decode the full track to interleaved stereo f32 in RAM, normalize the
   channel layout, and install it as an `Arc<DeckBuffer>` via command handoff.
4. **Mix (RT):** the callback reads one stereo frame per deck from the buffer using the fractional
   play-head, runs the per-deck processing graph, sends the result to the selected buses, sums the
   master, and writes to the cpal buffer (converted to the device sample format).
5. **Control:** fader/knob/transport changes arrive as `EngineMsg` → `AudioCommand`; parameter
   changes are smoothed (one-pole) on the RT side to avoid zipper noise.

Target per-deck graph (local decks):

```
source buffer
  -> play-head / rate conversion / scratch
  -> key-lock
  -> pregain / ReplayGain
  -> EQ / filter
  -> FX chain
  -> channel fader / crossfader assign
  -> buses (master, cue/headphones, booth, record)
```

The graph should become explicit and modular, but never dynamic on the callback in a way that
allocates or changes ownership. The control thread builds or mutates graph state; the RT side reads
preallocated processors, smoothed parameters, and stable buffer references. The detailed stage
contract and migration plan live in `docs/DECK-GRAPH.md`.

## 5a. Routing, devices, and reliability backlog

The current mixer exposes master output, headphone/PFL output, booth output, recording, and
telemetry. `compas-audio::Mixer` groups the secondary taps under `OutputRouting`: record,
cue/headphones, and booth each have one explicit owner. Secondary outputs are fed by lock-free
rings from the one RT mixer so decks and DSP are never double-advanced: cue/headphones receive a
PFL/master blend, booth receives the post-master mix with its own smoothed gain, and recording taps
the post-master mix for the writer thread. Cue and booth streams also publish measured CPAL device
latency plus their known prime-buffer latency through atomic `MonitorLatency` probes exposed via
`engine_status`.
The next hardening pass should make these guarantees explicit:

1. **Sync edge-case tests:** cover empty decks, late/early beatgrid anchors, tempo extremes,
   leader reassignment, paused leaders, loop-roll/slip interactions, and four-deck sync conflicts.
2. **Device hot-plug/recovery:** output devices can disappear or change sample rate/buffer size.
   The engine should report the failure, fall back to silence or the default device, and recover
   without hanging the UI or callback.
3. **Underrun/overload counters:** keep separate counters for stream underrun, callback over-budget,
   command-ring full, telemetry drop, cue-ring underrun, and record-ring overflow.
4. **Booth output:** optional third output stream exists with independent gain/device selection,
   fed from the post-master mix unless a future preference says otherwise.
5. **Master/headphone/record routing:** define routing as buses, not one-off taps. Recording should
   choose master or pre-master where supported; headphones should remain PFL/cue-aware; booth should
   stay independent from both.
6. **Latency compensation:** master, cue, and booth device/buffer latency are observable. Next,
   apply those offsets where needed so play-heads, sync, cue, and recordings align intentionally.
7. **No-drop RT guarantee:** any old `Arc<DeckBuffer>` or large processor state retired by load,
   eject, stem swap, or graph mutation must be reclaimed off the audio thread. Current deck/sample
   buffers use a reclaim ring plus bounded RT-side parking; future graph snapshots should use the
   same retire path or an equivalent preallocated handoff.
8. **Controller mapping profiles:** profile coverage should expand device-by-device with tests that
   every binding targets a real control, and hot-plug should reactivate the matching profile.
9. **Modular deck graph:** implement the target graph above so stems, ReplayGain, FX chains, booth,
   and future mic/aux routing can be added without threading ad hoc taps through the mixer.

## 6. Streaming integration (Phase 2) & licensing posture

- **Auth:** Authorization Code **+ PKCE** for Spotify & SoundCloud (public client, no secret),
  token exchange/refresh in the Rust process, tokens in the **OS keychain**. Apple Music uses an
  ES256 developer token. The auth uses a public-client (PKCE) transport so no secret ships in
  the distributable binary.
- **Playback:** the service SDK runs in the WebView; Rust sends transport commands via IPC. No
  PCM crosses into the engine.
- **Analysis gap:** Spotify `audio-features`/`audio-analysis` are deprecated for new apps, so
  streaming decks generally have **no beatgrid**. Auto-mix (P3) for streaming decks is limited to
  position-/metadata-timed transitions, not beat-sync.
- **Decode dependency licensing:** `symphonia` (MPL-2.0, permissive) is the default decoder.
  `ffmpeg-next` (LGPL/GPL) is a *fallback only* for genuine format gaps, dynamically linked,
  built without GPL components, and documented when used. Time-stretch / key-lock and beat/key
  detection are implemented **in-house** (a pure-Rust, RT-safe WSOLA stretcher and our own
  analysis), so they add no third-party DSP dependency. See `ROADMAP.md` for the dependency table.

## 7. IPC design (Tauri)

- **Commands (frontend → Rust):** typed `#[tauri::command]`s. P0 ships `app_info`,
  `set_crossfader`, `set_master_gain`, `set_deck_gain`. P1 adds `load_local_track`,
  `deck_transport`, `set_deck_eq`, `set_deck_filter`, `seek`, `analyze_track`, `engine_status`.
- **Events (Rust → frontend):** position/VU/underrun telemetry pushed on a timer (e.g. 30–60 Hz)
  via Tauri events; waveform peak data sent once per load. The audio thread never emits events
  directly — it writes to atomics/rings that the control thread samples and forwards.
- **Capability gating:** every deck exposes `SourceCapabilities` to the UI; controls bind their
  enabled-state to it, so streaming decks visibly disable DSP they can't do.
- **Type sharing:** TS mirrors of core types live in `apps/compas-dj/frontend/src/types`. A later step can
  codegen them from Rust (`ts-rs`/`specta`) to prevent drift.

## 8. Real-time safety rules (enforced in review)

1. No allocation, locks, syscalls, logging, or unbounded loops in the audio callback.
2. No `unwrap`/`expect`/`panic` on the audio path; underruns degrade to silence.
3. Cross-thread audio data moves only through `rtrb` SPSC rings; control changes only through the
   command ring (coalesce rapid UI changes; the ring can report "full").
4. Coefficient math (transcendentals) runs on the control thread; the RT side consumes prebaked
   coeffs and smooths targets.
5. Every RT-safe function carries an `RT-SAFE` doc-comment stating the contract; every offline
   function says so explicitly.
6. DSP/analysis units are unit-tested (`compas-dsp`, `compas-sources` already have tests).
