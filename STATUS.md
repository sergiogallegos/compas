# compas — status & resume point

> Checkpoint for picking work back up. Last updated: 2026-06-19. See `ROADMAP.md` for the
> full plan + **competitive feature backlog** (the source of truth for what's next),
> `CHANGELOG.md` for history, `AGENTS.md` for conventions.

## ▶ Resume here (next up, in order)
Big push this session: key-lock, continuous tempo+phase **SYNC**, **auto-mix**, master
**recording**, RT-load meter, manual grid nudge, **synth instrument + MIDI input**, **4-deck**,
centered-platter UI. All user-tested and confirmed working. Next, from the ROADMAP backlog:

1. **MIDI-learn / mapping** (+ Akai MPK Mini MK3 profile) — bind controller knobs/pads/keys to deck
   controls (EQ/filter/xfader/transport/cues/loops). Builds on the existing MIDI input (`midir`;
   `midi:cc` already emitted). User has an **Akai MPK Mini MK3** to test with (not always on hand).
2. **SQLite track DB + saved cues/loops** — persist cues, loops, beatgrids, gain, history across
   sessions. Foundation for real library management (`rusqlite`/`sqlx-sqlite`).
3. **Headphone / cue monitoring** — pre-listen the next track on a 2nd output (2nd cpal stream +
   cue bus). Biggest "real DJ" gap.
4. **Stem separation** — marquee 2025-26 feature, **needs an architecture decision first** (ONNX
   runtime + a Demucs-class model: bundle / optional-download / defer). Doesn't fit the pure-Rust
   ethos cleanly — discuss before starting.
5. **Performance layer:** sampler/pads, more + beat-synced FX, beat-jump/loop-roll/slip, quantize,
   harmonic-mixing assist (we already detect Camelot key).
6. **Infra (for release):** auto-update (`tauri-plugin-updater`) + code-signing/notarization;
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
  synth, knobs → `midi:cc`). Recordable. MIDI-learn / control mapping is the next step.
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
- **Local library** (add files → persisted; search; load A/B / double-click; remove) + load
  progress feedback.
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
