# compas — status & resume point

> Checkpoint for picking work back up. Last updated: 2026-06-19 (SQLite track DB landed). See `ROADMAP.md` for the
> full plan + **competitive feature backlog** (the source of truth for what's next),
> `CHANGELOG.md` for history, `AGENTS.md` for conventions.

## ▶ Resume here (next up, in order)
Latest: **SQLite track DB + saved cues/loops** landed (`rusqlite` bundled; `db.rs` schema +
WAL + FK cascade, unit-tested). Hot cues, last loop, grid nudge, gain persist + restore on
reload; BPM/key cached + shown in library; play count/history recorded. Old localStorage library
auto-migrates once. Builds clean (cargo check/clippy/test, tsc, vite) but **not yet runtime-tested
in the app** — when resuming, do a quick manual pass: add tracks, set cues/a loop, reload the
track, confirm they come back; check the DB at `%APPDATA%/<bundle-id>/compas.db`.

Before MIDI-learn, also still **not hardware-tested**: when the MPK Mini MK3 is on hand, connect
in the MIDI-mapping panel (sliders icon), LEARN a knob/pad, confirm it drives the bound control
and pads don't honk the synth while the instrument panel is closed.

Next, from the ROADMAP backlog:

1. **Headphone / cue monitoring** — pre-listen the next track on a 2nd output (2nd cpal stream +
   cue bus). Biggest "real DJ" gap; the mixer's headphone button is already stubbed.
2. **Stem separation** — marquee 2025-26 feature, **needs an architecture decision first** (ONNX
   runtime + a Demucs-class model: bundle / optional-download / defer). Doesn't fit the pure-Rust
   ethos cleanly — discuss before starting.
3. **Performance layer:** sampler/pads, more + beat-synced FX, beat-jump/loop-roll/slip, quantize,
   harmonic-mixing assist (we already detect Camelot key).
4. **Infra (for release):** auto-update (`tauri-plugin-updater`) + code-signing/notarization;
   `vergen` version string in the status bar.

**Optional polish (tune by ear):** scratch release-inertia + platter mapping; FX curves
(echo depth, reverb `WET_SCALE`); key-lock `STRETCH_WINDOW` (2048 ≈ 43 ms latency); sync PLL
gain (`SYNC_PHASE_GAIN`); auto-mix `TRANSITION_BEATS`/`LEAD_BEATS`.

## ✅ Done
**P0 scaffold** · Tauri 2 workspace, 4 engine crates, CI-green.

**P1 — local dual-deck engine (functionally complete):**
- Decode (symphonia, in-RAM `DeckBuffer`), 2 decks, transport (play/pause/cue/seek), crossfader,
  per-deck gain, 3-band EQ, HPF/LPF filter, varispeed + fine tempo trim.
- **Continuous beat-sync** — SYNC toggle holds a follower locked to a master (audio-thread PLL:
  tempo rate-match + ±8% phase-lock), composes with key-lock/loops.
- **Auto-mix** — AUTO (auto-transition near track end) + MIX (now): cue→sync→16-beat crossfade
  with bass swap→stop outgoing. Frontend orchestration (`useAutoMix`) over sync/crossfader/EQ.
- **Synth instrument + MIDI** — polyphonic synth (`Synth`: 4 waveforms, ADSR, 16 voices) on the
  master bus; on-screen keyboard + computer keyboard + MIDI controller input (`midir`; notes →
  synth, knobs → `midi:cc`). Recordable.
- **MIDI-learn / control mapping** — per-target LEARN binds any CC/note to deck + mixer controls
  (gain/EQ/filter/tempo/play/cue/sync/key-lock/hot-cues/loops/FX + crossfader); bindings persist
  to localStorage. Frontend mapping layer (`lib/midiMap.ts`, `hooks/useMidi.ts` +
  `hooks/useMidiMap.ts`, `components/MidiMap.tsx`) over engine `midi:note` events; `set_midi_synth`
  gates note→synth routing (instrument panel owns it). One-click **Akai MPK Mini MK3** starter
  profile (knobs CC 70–77, pads notes 36–43 — re-learn to match a specific unit).
- **4-deck mixing** — A/C + B/D switching slots, 4-channel mixer with per-deck crossfader-assign
  (A/thru/B); engine routes each deck to a crossfader side (`SetDeckXfaderAssign`).
- **Key-lock (master tempo)** — hand-rolled RT-safe WSOLA stretcher in `compas-dsp` (Hann grains
  + similarity search, reads from the in-RAM buffer, ~4%/core/deck); per-deck `KEY` toggle.
- **BPM + beatgrid + musical key** (Camelot) analysis on load.
- **Beat loops** (IN/OUT manual + 4/8/16 grid-snapped; waveform loop band).
- **Hot cues** (set/jump/clear).
- **Jog-wheel scratch** — draggable spinning platter drives the audio-thread read-rate from drag
  velocity (forward/reverse scrub + hold), engine `SetScratch`/`deck_scratch`, disc tracks the
  hand 1:1 (DSP/local decks only).
- **FX rack — echo/delay + reverb** — RT-safe `Delay` (pre-allocated ring, fractional read +
  time-glide, feedback/mix) and Schroeder/Moorer-style `Reverb` (8 combs → 4 allpass per channel,
  pre-allocated). Per-deck inserts post-EQ (`SetDeckEcho`/`SetDeckReverb`); UI: echo toggle +
  beat chips + DEPTH, reverb toggle + SIZE/MIX. FILTER stays the mixer knob.
- **Master recording** — record the master mix to a 32-bit-float stereo WAV (audio-thread tap →
  lock-free ring → writer thread; `start_recording`/`stop_recording`), title-bar REC toggle.
- Scrolling **zoom waveforms** (fixed NOW, beat grid, 4–32 s), VU metering; **manual
  beatgrid-anchor nudge**. **RT-load + xrun meter** in the title bar.
- **Local library + SQLite track DB** (`rusqlite` bundled; `db.rs`) — library persists in
  `compas.db` (app-data dir); add/search/load A·B/double-click/remove. Per-track state written
  through and restored on reload: hot cues, last loop, manual grid nudge, gain trim. Analysis
  (BPM/key) cached on load + shown in the list; play count + history recorded on first play;
  one-time migration from the old localStorage library. Unit-tested round-trips + FK cascade.
- Full performance UI (frameless window + traffic-light controls), brand mark + icons.

**P2a — Spotify (built, then parked):** Authorization Code + PKCE auth + catalog search exist in
code (`src-tauri/src/spotify.rs`, `frontend/src/lib/spotify.ts`, `useSpotify.ts`) but the UI
sources are **disabled** per request. See "Parked" below.

**Infra / OSS:** MIT `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AGENTS.md`, `rust-toolchain.toml`,
`rustfmt.toml`, CI (`.github/workflows/ci.yml`: fmt/clippy/test/frontend/audit), `release.yml`
(Win/macOS installers on `v*` tag; signing commented), `audit.toml`, criterion DSP benches,
`website/` landing page, test-WAV generator (`scripts/make-test-audio.mjs`).

## ⏸ Parked / known
- **Streaming (Spotify/Apple/SoundCloud)** disabled in the UI by request (focus on local). Spotify
  **connect didn't complete** last session — most likely the Spotify app's redirect URI isn't an
  exact match for `http://127.0.0.1:14565/callback`; verify that first when resuming P2.
- **Spotify 2b (playback)** not built — open question whether the Web Playback SDK runs in WebView2
  (Widevine) or we use the Spotify Connect remote-control fallback.
- LF→CRLF git warnings on Windows are harmless (could add `.gitattributes` to silence).

## Decisions (locked)
Varispeed default (key-lock later) · WASAPI shared for P1 · Apple Music deferred · MIT license ·
in-RAM deck model w/ cubic-Hermite play-head · **capability-honest UI** (streaming = control-only,
DSP locked) · streaming auth = PKCE (no secret).

## Run / verify
```bash
cargo test -p compas-dsp -p compas-audio              # engine unit tests
cargo clippy --all-targets -- -D warnings             # engine lint
cargo check --manifest-path src-tauri/Cargo.toml      # the Tauri app crate (separate from default-members)
cd frontend && npm install && npx tsc --noEmit && npx vite build
node scripts/make-test-audio.mjs                      # 120/128 BPM test WAVs -> samples/
```
**Launching the app (Windows, this machine):** `cargo tauri dev` is NOT installed; the working
command is the local Tauri CLI **from the repo root** (so it finds `src-tauri/`, and its
`beforeDevCommand` runs Vite in `frontend/`):
```bash
./frontend/node_modules/.bin/tauri dev
```
If it errors with "Port 5173 already in use", a previous Vite lingered — kill the PID listening on
5173 (and any stray `compas.exe`) first. The legacy-PowerShell-profile `Set-PSReadLineOption` error
that prints on npm/pwsh calls is harmless noise.
