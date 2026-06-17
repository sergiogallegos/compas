# compas — Roadmap

> Living document. Keep phase status and the dependency table current.

Status legend: ✅ done · 🔨 in progress · ⬜ not started

---

## Phase 0 — Scaffold ✅ (this commit)

- ✅ Cargo workspace + four engine crates (`compas-core`, `compas-dsp`, `compas-audio`,
  `compas-sources`), all compiling, `clippy`-clean, with unit tests passing.
- ✅ Real-time-safe DSP primitives (biquad EQ, equal-power crossfader, gain smoothing) + offline
  analysis scaffolds (spectral-flux onset; BPM/key stubs with TODOs).
- ✅ Audio engine skeleton: cpal output stream, lock-free command + per-deck PCM rings, N-deck Mixer.
- ✅ `AudioSource`/`PcmSource` abstraction with the local/streaming split enforced by types.
- ✅ Tauri 2 app shell (compiles; window opens; engine thread starts and degrades gracefully with
  no audio device) + React/Vite/TS frontend shell with a working IPC bridge and capability-gated UI.
- ✅ `docs/djvibebar-review.md`, `ARCHITECTURE.md`, this file, and a build/run `README.md`.

## Phase 1 — Local-file dual-deck engine (MVP) 🔨

**Goal:** load two local files onto two decks, decode, play through cpal with independent
transport, mix through a crossfader + per-deck gain + 3-band EQ, render scrubbable waveforms,
detect BPM with a beatgrid, and demonstrate **manual beatmatch end to end**.

Concrete tasks:

1. **Decode→ring pipeline.** Decoder worker per deck: `LocalFileSource::next_chunk` → `rubato`
   resample to device rate → push to PCM ring with half-full throttling. Bounded latency; clean
   EOF; pause/seek drain semantics.
2. **Transport + seek.** `deck_transport(play/pause/cue)` and `seek(deck, ms)` IPC commands;
   sample-accurate position tracking surfaced to the UI via a 30–60 Hz event.
3. **Mixer wiring (real-time path).** Hook the existing `Mixer` (gain, 3-band EQ, equal-power
   crossfader, master) to live deck rings; fix the **PCM-ring reclaim** RT hazard
   (`TODO(P1)` in `mixer.rs`) with a return ring.
4. **Filter knob.** Add the per-deck HPF/LPF DJ filter (coeffs already in `compas-dsp::rt`).
5. **Waveform rendering (WebGL).** Compute multi-resolution peak/RMS data on load (Rust),
   stream to the frontend; render overview + zoomed scrubbable waveform on a **WebGL canvas**
   (not DOM). Playhead + scrub interaction.
6. **BPM + beatgrid.** Implement tempo estimation in `compas-dsp::analysis` (spectral-flux onset
   → autocorrelation/comb-filter → octave correction), produce a beatgrid (downbeat + phase);
   manual tempo nudge and grid-anchor editing.
7. **Manual beatmatch.** Varispeed (pitch+tempo together via `rubato`) on each deck so the user
   can match BPMs and nudge phase; verify against a metronome/click and two real tracks.
8. **Engine telemetry.** `engine_status` (sample rate, buffer size, per-deck underruns, position)
   for diagnostics and the latency story.
9. **Tests.** Unit tests for tempo estimation on synthetic click tracks; ring-buffer
   underrun/overrun behavior; EQ/filter frequency-response sanity.

Out of scope for P1: time-stretch with key-lock, sync engine, cue/loops, streaming, FX, MIDI.

## Phase 2 — Streaming integration ⬜

- Authorization Code **+ PKCE** for Spotify & SoundCloud (Rust-side exchange/refresh, OS-keychain
  storage); Apple Music ES256 developer token. Port search/metadata clients from djvibebar.
- Library browser with local + streaming sources; BPM/key columns (local only, honestly blank for
  streaming where no data exists).
- Streaming **playback-only decks**: SDK in the WebView, transport via IPC, **capability-gated UI**
  (DSP controls disabled + explained).
- Document the per-service ToS/PCM/analysis constraints in-app (personal-use posture).

## Phase 3 — Auto-mix / intelligent transitions ⬜

- Local↔local: true beat-synced transitions using our beatgrids (the strong case).
- Streaming decks: **position/metadata-timed** transitions only (no beat data for new Spotify
  apps; no PCM). Be explicit in the UI about which kind of transition is happening.
- Transition planner (the "agentic" angle): pick in/out points, EQ swaps, tempo ramps.

## Phase 4 — Cue/loops/hot cues + sync engine hardening ⬜

- Cue points, beat loops (in/out, 1/2/4/8-beat), hot cues; quantize to beatgrid.
- Master-clock sync engine (tempo + phase), 2→4 deck fader/assign matrix.

## Phase 5 — Stems / FX / recording ⬜

- Stem separation (evaluate permissive models; licensing + latency review before committing).
- Effects rack (delay/reverb/filter/echo) on the local DSP bus; master recording.

## Phase 6 — MIDI controller mapping / hardware ⬜

- MIDI learn + mapping engine; common controller profiles; jog-wheel/scratch latency tuning.

---

## Dependency licensing table (things we actually link)

| Dependency | Role | License | Verdict |
|---|---|---|---|
| `tauri` / `tauri-build` | app shell | MIT/Apache-2.0 | ✅ |
| `cpal` | audio I/O (WASAPI/CoreAudio/ALSA) | MIT/Apache-2.0 | ✅ |
| `symphonia` | local decode (default) | **MPL-2.0** | ✅ permissive (file-level copyleft only) |
| `rtrb` | lock-free SPSC rings | MIT/Apache-2.0 | ✅ |
| `rubato` | resampling / varispeed | MIT | ✅ |
| `rustfft` | offline FFT (analysis) | MIT/Apache-2.0 | ✅ |
| `signalsmith-stretch` (planned, P1/P5) | time-stretch + key-lock | **MIT** (C++ via FFI) | ✅ preferred over Rubber Band |
| `ffmpeg-next` (fallback only) | decode gap coverage | **LGPL/GPL** | ⚠️ dynamic-link, LGPL build, no GPL components, documented |
| `keyring` (P2) | OS keychain for tokens | MIT/Apache-2.0 | ✅ |

**Reference-only (read to learn; never linked/copied — GPL):** Mixxx, Rubber Band, aubio, VLC
(LGPL, reference for I/O/clock patterns). Copying or statically linking GPL code would impose GPL
on compas; we do not.

**Patent note:** MP3 patents have expired. AAC patents may still apply to the *codec*; symphonia's
AAC/ALAC coverage is also partial — another reason the FFmpeg fallback decision is documented.

## Decisions made (2026-06-17)

- **Apple Music: deferred.** P2 ships Spotify + SoundCloud control-only decks. Apple Music is
  revisited later if wanted (avoids the extractable-`.p8` problem and trims scope).
- **Beatmatch: varispeed by default, key-lock as a toggle.** Tempo+pitch move together (vinyl
  feel, `rubato`); key-lock (tempo-independent pitch, `signalsmith-stretch`) is opt-in.
- **Windows output: WASAPI shared mode, safe buffers (~10–20 ms) for P1.** Low-latency
  exclusive/ASIO is a later optimization. macOS uses CoreAudio.

## Open questions (remaining; lower-stakes, sensible defaults assumed)

- **Local library storage** — default assumption: **SQLite** (via `rusqlite`/`sqlx-sqlite`) for the
  track DB, cues, beatgrids. Speak up if you'd prefer flat files/JSON.
- **Waveform renderer** — default assumption: **raw WebGL** (thin custom layer) over a heavier lib
  (PixiJS/regl), to keep the audio-rate render path lean. Open to PixiJS if you want batteries.
- **Type sharing** — default assumption: hand-maintained TS mirrors now, adopt `ts-rs`/`specta`
  codegen once the command surface grows in P1/P2.
