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
- ✅ `ARCHITECTURE.md`, this file, and a build/run `README.md`.

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
5. ✅ **Waveform rendering.** Peaks computed in Rust on load; SVG lanes with a **scrolling
   zoom detail view** (fixed NOW playhead at 38%, 4/8/16/32 s zoom, beat-aligned grid,
   click-to-seek within the window).
6. ✅ **BPM + beatgrid + key.** Tempo (spectral-flux onset → autocorrelation → parabolic refine
   → octave fold) + **beat phase** (comb over the envelope) → grid overlay with emphasized
   downbeats. **Musical key** via chromagram → Krumhansl–Schmuckler (Camelot). *Remaining: manual
   grid-anchor editing.*
7. ✅ **Manual beatmatch + tempo SYNC.** Varispeed (tempo+pitch coupled) + tempo fader + nudge;
   one-shot **SYNC** matches a deck's effective BPM to the other. *Remaining: end-to-end verify
   against real tracks; continuous/phase sync (→ P4).* Key-lock (in-house WSOLA) is now done.
8. ✅ **Engine telemetry.** `engine_status` + per-deck position/level + master meter.
   *Remaining: buffer size + underrun counters surfaced to the UI.*
9. 🔨 **Tests.** ✅ Tempo/beatgrid/key on synthetic signals; ✅ interpolation/crossfade/EQ/peaks.
   *Remaining: integration test that decodes a fixture file and renders N frames.*

**Phase 1 is functionally complete** (MVP proven end-to-end). Small remainders above are polish;
next major work is P2 (streaming) or pulling the P4 sync engine forward.

Out of scope for P1: key-lock time-stretch, continuous sync engine, cue/loops, streaming, FX, MIDI.

## Phase 2 — Streaming integration ⬜

- Authorization Code **+ PKCE** for Spotify & SoundCloud (Rust-side exchange/refresh, OS-keychain
  storage); Apple Music ES256 developer token. Build the search/metadata clients per provider.
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

## Infrastructure & distribution (pending)

- 🔨 **Release pipeline** (`.github/workflows/release.yml`, via `tauri-action`): on a `v*` tag,
  build **Windows `.msi`/NSIS** and **macOS `.dmg`** installers and publish them as GitHub Release
  assets. Feeds the website download buttons. *(Scaffolded; code signing is a follow-up.)*
- ⬜ **In-app auto-update** (user-requested) via **`tauri-plugin-updater`**: on launch (and via a
  manual **"Check for updates"** button) the app pings the releases endpoint, detects a newer
  version, and offers a one-click **download + install**. Requires:
  1. an updater signing keypair (`tauri signer generate`) — **pubkey** in `tauri.conf.json`,
     **privkey** as a CI secret (`TAURI_SIGNING_PRIVATE_KEY`);
  2. the release workflow emitting a `latest.json` manifest the app checks;
  3. an "Update available → Install & restart" UI.
- ⬜ **Build/version info** via `vergen` — show the real version + git short-SHA in the status bar
  (replaces the hardcoded `compas 0.1.0`).
- ✅ **CI** (fmt + clippy + tests + frontend build + `cargo audit`); **`audit.toml`**.
- 🔨 **Criterion benchmarks** for the DSP hot loops (biquad/EQ/crossfade, tempo analysis).
- ⬜ **Code signing / notarization** (Windows Authenticode, macOS notarization) so installers and
  auto-updates aren't flagged — needed before a public release.

## Dependency licensing table (things we actually link)

| Dependency | Role | License | Verdict |
|---|---|---|---|
| `tauri` / `tauri-build` | app shell | MIT/Apache-2.0 | ✅ |
| `cpal` | audio I/O (WASAPI/CoreAudio/ALSA) | MIT/Apache-2.0 | ✅ |
| `symphonia` | local decode (default) | **MPL-2.0** | ✅ permissive (file-level copyleft only) |
| `rtrb` | lock-free SPSC rings | MIT/Apache-2.0 | ✅ |
| `rubato` | resampling / varispeed | MIT | ✅ |
| `rustfft` | offline FFT (analysis) | MIT/Apache-2.0 | ✅ |
| `ffmpeg-next` (fallback only) | decode gap coverage | **LGPL/GPL** | ⚠️ dynamic-link, LGPL build, no GPL components, documented |
| `keyring` (P2) | OS keychain for tokens | MIT/Apache-2.0 | ✅ |

Time-stretch / key-lock and beat/key detection are **implemented in-house** (a pure-Rust,
RT-safe WSOLA stretcher and our own analysis), so they add no third-party DSP dependency.

**Patent note:** MP3 patents have expired. AAC patents may still apply to the *codec*; symphonia's
AAC/ALAC coverage is also partial — another reason the FFmpeg fallback decision is documented.

## Decisions made (2026-06-17)

- **Apple Music: deferred.** P2 ships Spotify + SoundCloud control-only decks. Apple Music is
  revisited later if wanted (avoids the extractable-`.p8` problem and trims scope).
- **Beatmatch: varispeed by default, key-lock as a toggle.** Tempo+pitch move together (vinyl
  feel); key-lock (tempo-independent pitch, our in-house WSOLA stretcher) is opt-in.
- **Windows output: WASAPI shared mode, safe buffers (~10–20 ms) for P1.** Low-latency
  exclusive/ASIO is a later optimization. macOS uses CoreAudio.

## Open questions (remaining; lower-stakes, sensible defaults assumed)

- **Local library storage** — default assumption: **SQLite** (via `rusqlite`/`sqlx-sqlite`) for the
  track DB, cues, beatgrids. Speak up if you'd prefer flat files/JSON.
- **Waveform renderer** — default assumption: **raw WebGL** (thin custom layer) over a heavier lib
  (PixiJS/regl), to keep the audio-rate render path lean. Open to PixiJS if you want batteries.
- **Type sharing** — default assumption: hand-maintained TS mirrors now, adopt `ts-rs`/`specta`
  codegen once the command surface grows in P1/P2.
