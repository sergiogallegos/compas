# compas — status & resume point

> Checkpoint for picking work back up. Last updated: 2026-06-20 (release infra). See `ROADMAP.md` for the
> full plan + **competitive feature backlog** (the source of truth for what's next),
> `CHANGELOG.md` for history, `AGENTS.md` for conventions.

## ▶ Resume here (next up, in order)

**Post-12-features build-out (2026-06-20).** After the 12 design-study features landed, four phases
were taken on (per the maintainer's order), all committed on `main`, each step tested:
- **Phase 1 — UI + website ✅ done.** 8 UI batches wiring every feature to React controls
  (crossfader curve/cut/reverse, FX macro, loop ½×/2×/move, cue modes + tempo-only sync + leader,
  band-RGB waveform, latency-aware play-head, library grammar search + ✨ suggest-next, crates panel)
  + `compasaudio.com` CNAME/social cards. Frontend builds clean.
- **Phase 2 — `trait Effect` FX rack ✅ done.** `Effect` trait + reorderable `FxChain` (step A),
  then the deck's fixed inserts swapped onto the chain, behavior-preserving (step B).
- **Phase 3 — JS scripting sandbox + host wiring ✅.** `compas-script` crate (QuickJS via `rquickjs`,
  quarantined): sandboxed `engine.*` API + `onMidi`, 6 tests. **Host-wired:** a controller-engine
  thread owns the runtime + active profile; MIDI is forwarded to it; it resolves and emits
  `controller:update` events the frontend applies via the existing setters. **Remaining:**
  `engine.sendMidi` LED feedback, in-app script editor.
- **Phase 4 — controllers 🔨 core done.** `docs/CONTROLLER-ARCHITECTURE.md` + `docs/CONTROLLERS.md`
  (the ~140-device target matrix, clean-room) + serde `ControllerProfile` (`compas-core::mapping`) +
  the **profile loader** (`controllers` backend module: list/load/save + IPC) + the **controller
  engine** (declarative bindings w/ soft-takeover → control updates; script fallback) + a frontend
  controller-bus dispatcher + activate/deactivate IPC. **Remaining:** clean-room starter MIDI
  profiles (priority list in `docs/CONTROLLERS.md`), output/LED feedback, the guided learn editor,
  and HID input (`hidapi`).

Other deferred follow-ups (flagged in commits): FX internal-clock virtual leader; library
OR-search/smart-crates/tags/folder-watch; full AutoDJ queue; stem-separation S1 resampling +
optional-download. Brand stays **compas**; domain **compasaudio.com** (Cloudflare/Pages DNS is the
account-side step).


**Design-study feature batch — engine + IPC complete (2026-06-20).** All 12 items from the
ROADMAP deep-dive backlog are implemented at the **Rust-engine + IPC + TS-binding** level, each
unit-tested and committed on `main` (one `feat(...)` commit per feature). Verified together:
`cargo test` (all crates green), `cargo clippy -D warnings` (default-members), `cargo check`
(Tauri app), `tsc --noEmit` (frontend) — all clean.
1. ✅ Typed control bus (`compas-core::control`) · 2. ✅ Crossfader curve/additive/reverse ·
3. ✅ Main-cue modes (CDJ/gated) · 4. ✅ Loop scale + move · 5. ✅ Sync tempo-only/explicit-leader/
ranked picker · 6. ✅ ReplayGain (auto on load) · 7. ✅ FX meta-knob + link types · 8. ✅ Band-RGB
waveform analysis · 9. ✅ DAC-latency-aware play-head data · 10. ✅ Library search grammar +
crates/playlists · 11. ✅ Auto-mix harmonic+tempo planner (`compas-core::automix`) ·
12. ✅ Declarative mapping/soft-takeover (`compas-core::mapping`).

**What's deliberately deferred (next):**
- **One consolidated UI pass** wiring these to React controls (FX-macro knob, cue-mode toggle, loop
  buttons, band-RGB waveform render via `bandColor`/WebGL, latency-aware play-head via
  `extrapolateFrame`, crate/playlist browser + search box, "suggest next" panel, mapping editor),
  **plus the website pass** (point `compasaudio.com` via Cloudflare; brand stays **compas**).
- **Larger follow-ups noted in commits:** full chainable `trait Effect` FX rack (replacing fixed
  inserts), sync internal-clock virtual leader, embedded JS sandbox (`rquickjs`/`boa`) for true
  scripting, OR-between-terms + smart crates + tags + folder-watch, full AutoDJ queue.

**Stem separation (S1)** remains as previously noted (first slice done; resampling +
optional-download follow-ups).

**User-confirmed working in the running app (2026-06-19):** the user ran `tauri dev` and reported
the recent batch works — FX rack (echo/reverb/flanger/bitcrusher), performance row (quantize /
beat-jump / loop-roll), sampler pads, headphone cue, and A/B/C/D library loading. (Hardware MIDI
with the MPK MK3 still needs the controller on hand; the rest is confirmed.)

Most recent feature was the **Bitcrusher FX** (CRUSH chip), preceded by the flanger, MIDI-mapped
sampler pads, the sampler, the perf row, cue monitoring, the SQLite DB, MIDI-learn, and 4-deck —
all described in the **Done** section below. SQLite was also verified end-to-end via a live DB
query (migration + analysis-cache + play history).

**Only remaining unverified item:** **MIDI with real hardware** — when the Akai MPK Mini MK3 is on
hand, load its starter profile (or LEARN), check knobs drive the mixer + pads fire the sampler,
and that pads don't honk the synth while the instrument panel is closed.

Next, from the ROADMAP backlog:

1. **Stem separation** — marquee 2025-26 feature. **Architecture decided 2026-06-20** (see
   `ROADMAP.md` § "Decisions made (2026-06-20)"): offline pre-computed stems (4 buffers/deck,
   mixed at playback) · `ort` (ONNX Runtime) in a new quarantined `compas-stems` crate · htdemucs
   model · optional-download on first use. **Implementation is next, in three slices:**
   - **✅ ONNX-export spike (2026-06-20) — PASSED.** Single-file **htdemucs** ONNX exists and
     auto-downloads from HF (`StemSplitio/htdemucs-onnx`, **301 MB fp32**, fp16-weights variant ~half).
     STFT is **inside** the model → IO is trivial: input `mix` `[1,2,343980]` f32 (a **fixed 7.8 s**
     segment @ 44.1 k), output `stems` `[1,4,2,343980]` f32 (drums/bass/other/vocals). Verified on the
     synthetic test WAV (kick+bass, no vocals): RMS landed in drums(.17)+bass(.08), vocals(.0002)+
     other(.002) ~silent — correct routing. Tooling: `demucs-onnx` PyPI (0.3.4) for export+ref
     inference; `sevagh/demucs.onnx` is the C++/ORT reference for segmentation + overlap-add + the
     mean/std normalization to port. Rust runtime = **`ort` 2.0.0-rc.12** (wraps the same ONNX Runtime;
     DirectML/CoreML/CUDA EPs available). Spike artifacts in `~/stem-spike` (outside the repo).
   - **🔨 S1 — offline pipeline (first slice DONE 2026-06-20).** New **`compas-stems`** crate
     (deps: `ort` 2.0.0-rc.12 + `thiserror`/`tracing`; a workspace member but **not** in
     `default-members`, so core CI stays pure). Implemented: `StemSeparator::{load,separate}`,
     the **segmented overlap-add** core (7.8 s / `N_SAMPLES=343980` segments, ¼ overlap, triangular
     window, weight-normalized), interleave/deinterleave. Note: the single-file htdemucs graph bakes
     STFT **and** mean/std normalization inside, so the host does **no** normalization — just chunk +
     window + overlap-add. **Verified:** `cargo test`+`clippy` green; the live `ort` smoke test
     (`-- --ignored` with `COMPAS_HTDEMUCS_ONNX=<cached htdemucs.onnx>`) loads the real 301 MB model
     and runs a `[1,2,343980]`→`[1,4,2,343980]` frame in ~4.6 s — **Rust path proven**.
     **Remaining S1 follow-ups:** rubato resampling for non-44.1 kHz sources (today `separate` errors
     on a rate mismatch via `StemError::UnsupportedRate`); checksum'd optional-download of the model
     (HF `StemSplitio/htdemucs-onnx` or our own mirror) into the app-data dir; switch `ort` to
     `load-dynamic` so the runtime ships via that download path.
   - **S2 — engine integration:** deck holds `Option<[Arc<DeckBuffer>; 4]>`; mixer reads 4
     play-heads × 4 gains (RT-safe, same play-head math); `AudioCommand::SetDeckStemGain` +
     `separate_stems`/`set_deck_stem` IPC (separation job emits progress, results cached to disk +
     referenced from the SQLite DB so reload is instant).
   - **S3 — UI:** per-deck STEMS panel (DRUMS/BASS/OTHER/VOCALS faders + mutes), a separate button
     with progress, and the first-use model-download prompt.
2. **More performance layer:** sampler/pads (reuse the synth voices), more + beat-synced FX,
   full global slip mode + reverse/censor, harmonic-mixing assist (we already detect Camelot key).
3. **Release infra — wiring done, secrets pending (2026-06-20).** Auto-update plugin, manual
   "check for updates" on the title-bar version chip, and a git-sha build chip all integrated;
   `release.yml` env block fully populated for `TAURI_SIGNING_*` + Apple notarization. Before
   the first signed release: run `npx tauri signer generate -w ~/.tauri/compas.key`, paste the
   pubkey into `src-tauri/tauri.conf.json` (replacing `REPLACE_BEFORE_RELEASE_…`), and add the
   matching repo secrets. See `CONTRIBUTING.md` § "Release setup".

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
- **Sampler / performance pads** — 8-pad sampler (`compas-audio::sampler`, 16-voice pool,
  RT-safe) on the master bus: load a file per pad, press to fire (one-shots overlap), per-pad
  loop toggle, global level. `load_sample`/`trigger_sample`/`clear_sample`/`stop_sample`/
  `set_sample_loop`/`set_sampler_gain`. Unit-tested. MIDI-mapping the pads is a follow-up.
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
- **Performance layer (round 1)** — per-deck **quantize** (snaps cue jumps + beat-jumps),
  **beat-jump** (±4 beats, grid-aligned), and **loop-roll** (held ⅛/¼/½) with **true slip**:
  engine `SetLoopRoll` keeps a shadow play-head advancing so release catches up to real time.
- **Hot cues** (set/jump/clear).
- **Jog-wheel scratch** — draggable spinning platter drives the audio-thread read-rate from drag
  velocity (forward/reverse scrub + hold), engine `SetScratch`/`deck_scratch`, disc tracks the
  hand 1:1 (DSP/local decks only).
- **FX rack — echo/delay + reverb** — RT-safe `Delay` (pre-allocated ring, fractional read +
  time-glide, feedback/mix) and Schroeder/Moorer-style `Reverb` (8 combs → 4 allpass per channel,
  pre-allocated). Per-deck inserts post-EQ (`SetDeckEcho`/`SetDeckReverb`); UI: echo toggle +
  beat chips + DEPTH, reverb toggle + SIZE/MIX. FILTER stays the mixer knob.
- **FX rack — flanger** — beat-synced stereo `compas-dsp::Flanger` (LFO-swept comb, quadrature
  L/R, feedback), per-deck insert after reverb; UI: FLANGE toggle + 1/2/4/8-beat rate chips +
  DEPTH. `SetDeckFlanger`/`set_deck_flanger`. Unit-tested.
- **FX rack — bitcrusher** — `compas-dsp::Bitcrusher` (bit-depth quantise + sample-and-hold
  rate reduction, no allocation), per-deck insert after flanger; UI: CRUSH toggle + BITS/RATE
  knobs. `SetDeckCrusher`/`set_deck_crusher`. Unit-tested.
- **Master recording** — record the master mix to a 32-bit-float stereo WAV (audio-thread tap →
  lock-free ring → writer thread; `start_recording`/`stop_recording`), title-bar REC toggle.
- **Headphone / cue monitoring (PFL)** — per-channel CUE buttons + a phones bar (device picker,
  ON/OFF, CUE◁▷MASTER blend, PHONES level). Mixer sums cued decks into a cue bus, blends with the
  master, pushes through a ring to a 2nd cpal output stream (`compas-audio::cue`) on its own
  thread (prime + re-prime on underrun for clock drift). `start/stop_cue_output`,
  `set_deck_cue`/`set_cue_mix`/`set_cue_volume`, `list_output_devices`. Unit-tested cue summing.
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
