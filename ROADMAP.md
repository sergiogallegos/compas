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

> **Playback model (decided during P1):** decks hold the **fully-decoded track in RAM**
> (`Arc<DeckBuffer>`) and the audio thread reads with a **cubic-Hermite fractional play-head**
> advancing by `(source_rate/device_rate) × tempo` per output sample. This replaced the
> streaming PCM-ring model — it makes seek/varispeed/scratch/loops instant and is the standard
> pro approach for local DJ tracks.

Concrete tasks:

1. ✅ **Decode→buffer pipeline.** `compas_sources::decode_full` decodes to an in-RAM stereo
   buffer on a worker thread; installed on a deck via `AudioCommand::LoadDeck`. (rubato resampling
   is deferred — the play-head's interpolation handles device-rate mismatch; high-quality offline
   resample is a later quality pass.)
2. ✅ **Transport + seek.** `deck_play`/`deck_pause`/`deck_seek` IPC commands; lock-free play-head
   telemetry published per audio block and emitted to the UI at 30 Hz (`deck:position`).
3. ✅ **Mixer wiring (real-time path).** Decks → 3-band EQ → filter → gain → equal-power crossfader
   → master, all live. **PCM-ring reclaim RT hazard fixed** via a reclaim ring (retired
   `Arc<DeckBuffer>`s are dropped on the control thread).
4. ✅ **Filter knob.** Per-deck bipolar HPF/LPF DJ filter (`set_deck_filter`).
5. 🔨 **Waveform rendering.** Peaks computed in Rust on load (`compute_peaks`) and drawn on a
   **WebGL canvas** (Canvas-2D fallback) with playhead + click-to-seek. *Remaining: zoom + a
   scrolling/beat-aligned detail view.*
6. 🔨 **BPM + beatgrid.** ✅ Tempo estimation (spectral-flux onset → autocorrelation → parabolic
   refine → octave fold), tested on a synthetic click track. *Remaining: downbeat/phase → an
   actual beatgrid overlay, and manual grid-anchor editing.*
7. 🔨 **Manual beatmatch.** ✅ Varispeed (tempo+pitch coupled) per deck via the play-head rate, with
   a tempo fader + momentary nudge in the UI. *Remaining: end-to-end verification against a click
   and two real tracks; key-lock toggle (signalsmith-stretch).*
8. ✅ **Engine telemetry.** `engine_status` (sample rate, per-deck loaded/playing/position).
   *Remaining: buffer size + underrun counters surfaced to the UI.*
9. 🔨 **Tests.** ✅ Tempo on synthetic click; ✅ interpolation/crossfade/EQ/peaks units.
   *Remaining: integration test that decodes a fixture file and renders N frames.*

Out of scope for P1: key-lock time-stretch, sync engine, cue/loops, streaming, FX, MIDI.

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
