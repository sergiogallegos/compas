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
   downbeats. **Musical key** via chromagram → Krumhansl–Schmuckler (Camelot). ✅ Manual
   grid-anchor nudge (waveform).
7. ✅ **Manual beatmatch + SYNC.** Varispeed (tempo+pitch coupled) + tempo fader + fine trim;
   **continuous tempo + phase SYNC** (audio-thread PLL). Key-lock (in-house WSOLA) is done.
8. ✅ **Engine telemetry.** `engine_status` + per-deck position/level + master meter + ✅
   audio-thread load / xrun counter surfaced in the title bar.
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
- Document the per-service ToS/PCM/analysis constraints in-app.

## Phase 3 — Auto-mix / intelligent transitions 🔨

- ✅ Local↔local: beat-synced auto-transition (cue → sync → 16-beat crossfade + bass swap →
  stop outgoing). AUTO (near track end) + MIX (now), via the frontend `useAutoMix` orchestrator.
- 🔶 AutoDJ queue: the library can enqueue tracks and load the next queued item to an empty deck
  (or deck B fallback). ⬜ Full unattended AUTO chaining and smarter planner UI: per-track in/out
  point selection, tempo ramps, EQ curves.
- ⬜ Streaming decks: **position/metadata-timed** transitions only (no beat data; no PCM).

## Phase 4 — Cue/loops/hot cues + sync engine hardening 🔨

- ✅ Cue points, **beat loops** (manual in/out + 4/8/16-beat, beatgrid-snapped), **hot cues**.
- ✅ Master-clock **sync engine** — continuous tempo **+ phase** lock (audio-thread PLL).
- ✅ **4-deck** — A/C + B/D switching slots, 4-channel mixer, per-deck crossfader assign.

## Phase 5 — Stems / FX / recording 🔨

- 🔨 **Stem separation** — **decided 2026-06-20** (see "Decisions made"): offline pre-computed
  stems via `ort` (ONNX Runtime) + htdemucs, optional-download model. S1 offline pipeline is
  implemented, including model-rate/source-rate resampling; model download + engine/UI integration
  remain.
- ✅ **Effects rack:** echo/delay + reverb on the local DSP bus (filter already existed).
- ✅ **Master recording** (master tap → lock-free ring → WAV writer thread).

## Phase 6 — MIDI controller mapping / hardware 🔨

- ✅ **Jog-wheel scratch** (draggable platter; drag velocity drives the audio-thread read rate).
- ✅ **MIDI input** (`midir`): connect a controller; notes drive the synth, CC emits `midi:cc`.
- ✅ **Synth instrument** (polyphonic, master bus, recordable) — on-screen + computer + MIDI keys.
- ✅ **MIDI-learn / mapping engine**: bind controller knobs/pads/keys to deck controls (EQ, filter,
  crossfader, transport, sync, key-lock, hot cues, loops, FX). Per-target LEARN + persisted
  bindings; one-click Akai MPK Mini MK3 starter profile. `midi:note` events + `set_midi_synth` gate.
- 🔶 Controller workflow: guided mapping exists, bundled MIDI/HID profile foundation exists, and
  the controller panel can connect MIDI + show live input while learning. ⬜ More hardware profiles,
  per-device HID button/LED work, sampled/instrument upgrades, scratch latency tuning.

> **Note:** features were pulled forward out of phase order. Beyond P1, what's *actually* shipped
> already includes key-lock, beat loops, hot cues, the echo/reverb FX rack, jog-wheel scratch, and
> master recording. The big remaining functional work is the **continuous/phase sync engine**, the
> **4-deck layout**, **MIDI mapping**, **stem separation**, and **auto-mix transitions** (P3).

---

## Competitive landscape & prioritized backlog (reviewed 2026-06)

How compas stacks up against Serato, rekordbox, Traktor, VirtualDJ, and Ableton Live, and
what's worth adding. Status: ✅ have · 🔶 partial · ⬜ missing.

### Where compas is already competitive
- ✅ Dual-deck mixing: equal-power crossfader, per-deck gain, 3-band EQ, HPF/LPF filter.
- ✅ Analysis: BPM, beatgrid, musical key (Camelot), scrolling + overview waveforms, manual grid nudge.
- ✅ **Key-lock** (master tempo, in-house WSOLA) · ✅ **continuous tempo+phase SYNC**.
- ✅ Beat loops + hot cues · ✅ jog-wheel **scratch** · ✅ **echo/reverb FX** (+ filter).
- ✅ **Auto-mix** with bass-swap transitions (Serato/VirtualDJ-style automix).
- ✅ **Master recording** · ✅ RT-load meter.
- ✅ **Synth instrument + MIDI input** — uncommon in DJ apps (Ableton-flavored); a differentiator.
- ✅ Capability-honest streaming model (parked).

### Gaps vs the field, prioritized

**Tier 1 — core DJ essentials still missing:**
- ✅ **4-deck layout** — A/C + B/D switching slots, 4-channel mixer, per-deck crossfader assign.
- ✅ **Headphone / cue monitoring** — per-channel PFL/CUE + a 2nd cpal output stream fed by a
  cue bus in the mixer; CUE↔MASTER blend + phones level, device picker.
- ✅ **MIDI-learn / controller mapping** — per-target LEARN binds any knob/fader/pad to deck +
  mixer controls (EQ/filter/xfader/transport/sync/key-lock/cues/loops/FX); bindings persist;
  one-click **Akai MPK Mini MK3** starter profile.
- ✅ **Saved cues/loops + a track database (SQLite)** — `rusqlite` store persists cues, the last
  loop, beatgrid nudge, gain, cached BPM/key, and play history; restored on reload. Foundation
  for real library management.
- ✅ **Quantize** — per-deck Q snaps hot-cue jumps + beat-jumps to the grid (loops already snap).

**Tier 2 — modern differentiators (what 2025-26 DJs expect):**
- 🔨 **STEM separation** (vocal/drum/bass/melody isolation) — table-stakes across
  Serato/rekordbox/Traktor/VirtualDJ/djay; biggest single feature gap. **Model decision made
  2026-06-20:** offline pre-computed stems (4 buffers/deck, mixed at playback) via `ort`
  (ONNX Runtime, quarantined native dep) + **htdemucs**, model fetched on first use
  (optional-download). True real-time separation deferred. See "Decisions made".
- 🔶 **More FX + beat-synced timing + FX units/chains** — echo, reverb, beat-synced flanger, and
  bitcrusher ship; phaser + FX chaining still open. (FX time already in beats.)
- ✅ **Sampler / performance pads** — 8 pads, one-shots (polyphonic) + per-pad loop toggle, global
  level; RT-safe `compas-audio::sampler` on the master bus (recordable). MIDI-mapping the pads is
  a follow-up.
- 🔶 **Beat jump / loop roll / slip mode / reverse-censor** — beat-jump (±bar) + held loop-roll
  *with slip* shipped; full global slip mode + reverse/censor still to do.
- ⬜ **Harmonic-mixing assist** — we already detect Camelot key; suggest key-compatible next tracks.

**Tier 3 — library & ecosystem:**
- ⬜ Playlists/crates, tags/rating, smart playlists, history/session export, metadata edit, folder watch.
- ⬜ Streaming services (unpark when ready; PKCE auth already built).
- ⬜ **Ableton Link / MIDI clock** — tempo-sync with Ableton and external gear.
- ⬜ Auto-gain / loudness normalization on analysis.

**Tier 4 — advanced / niche (later, maybe never):**
- ⬜ Video mixing, karaoke, DMX lighting · HID hardware (pro-controller jog wheels).

### Feature deep-dive backlog (design study, 2026-06-20)

A focused study of how mature desktop DJ software implements its engine, mixer, sync, effects,
waveforms, library, and controller layers surfaced the items below. They are framed as compas
features (clean-room — behaviors and concepts only, independently designed). Detailed write-ups,
KEEP/IMPROVE/DROP calls, and the full Rust-core ↔ TS-UI mapping were captured in internal design
notes.

> **Status (2026-06-20): all 12 implemented at the engine + IPC + TS-binding level**, each
> unit-tested and committed individually on `main`. Remaining: one consolidated **UI pass** to wire
> them to React controls, plus the per-item **follow-ups** flagged below (and in each commit).

**Near-term (the control bus enables much of the rest):**
1. ⬜ **Typed control bus** — a named `(group, param)` registry where every tweakable engine value
   is one shared, lock-free cell carrying a value↔normalized↔MIDI behavior curve. This is the seam
   that MIDI-learn, controller scripting, a skinnable UI, and batched UI telemetry all hang off;
   building it first unlocks several later items. *Rust core: registry + atomics/seqlock; UI: a
   single batched `controls:changed` event instead of many per-param events.*
2. ⬜ **FX chains + meta/super-knob** — move past fixed per-deck inserts to chainable effect units
   with a manifest-driven parameter model and a macro knob that drives many params via link types
   (linked / left / right / left-right / inverted / neutral-split). *Rust: `trait Effect` +
   state/processor split (allocation off the audio thread); UI: chain rack + macro mapping.*
3. ⬜ **Frequency-band RGB waveforms** — color the waveform by band energy (low/mid/high → RGB,
   normalized by the max component), with live EQ-gain color feedback. *Rust: band-split peak texels
   in the offline waveform pass; UI: one-draw-call WebGL shader.*
4. ⬜ **DAC-latency-aware playhead** — extrapolate the scrolling playhead against output latency +
   VSync so motion stays smooth and decoupled from buffer size. *UI render loop + engine latency clock.*
5. ⬜ **Crossfader curve + additive mode + reverse** — an adjustable transform curve, additive
   ("slow-fade / fast-cut") vs constant-power modes, and reverse; plus an anti-zipper ramp audit on
   every gain stage. *Rust core (mixer summing stage).*
6. ⬜ **Configurable main-cue modes** — selectable cue behaviors (preview-while-held,
   release-to-play, and other hardware-faithful modes) for muscle-memory parity. *Rust transport + UI setting.*
7. ⬜ **Full loop toolkit** — widen the beat-loop range (1/32…512), add a roll stack, loop
   move/scale/halve-double, live in/out drag with quantized snap, saved loops on hot cues, and
   seek-on-load modes. *Rust loop engine; UI waveform interactions.*
8. ⬜ **Sync coordinator hardening** — a central coordinator with soft (auto, reassignable) vs
   explicit (pinned) leaders, a virtual internal-clock leader, a pure ranking-based leader picker,
   and separate tempo-only / phase-only triggers. Refines today's PLL sync. *Rust core; unit-tested.*
9. ⬜ **ReplayGain / loudness normalization** — compute per-track gain on analysis and apply it in a
   pregain stage for consistent deck levels. *Rust analysis + pregain.*

**Mid-term:**
10. ⬜ **Library platform** — crates + ordered playlists, a real query language (fielded + fuzzy +
    bpm/key ranges + negation/OR), smart crates, free-form tags, typed/colored cue rows, and
    incremental directory-hash scanning. Builds on the existing SQLite store. *Rust: schema + query
    compiler (`rusqlite`), `notify` for folder watch; UI: browser + smart-crate builder.*
11. ⬜ **Agentic auto-mix / set construction** — a planner that picks in/out points, harmonically
    compatible next tracks (Camelot already detected), tempo ramps, and EQ curves, plus an AutoDJ
    queue for unattended track→track chaining. The headline differentiator. *Rust planner over
    analysis; UI queue + override.*
12. ⬜ **Sandboxed scripting layer** — a JS/TS controller-scripting runtime over the control bus
    (an `engine.*` API, declarative input + output/LED bindings, soft-takeover, a small std library,
    and an in-app guided-learn editor). Turns compas into an extensible platform. *Rust: embed an
    `rquickjs`/`boa` sandbox bound to the control bus; UI: mapping editor.*

**Later tiers:** compressed recording (FLAC/Opus) + a live cue-sheet; read-only library import from
other DJ apps; split-cue (mono cue / mono master) monitoring; fold EQ + filter into the chain
abstraction; Ableton Link / MIDI-clock sync.

### Suggested near-term order (step by step)
1. ✅ **4-deck layout** — done.
2. ✅ **MIDI-learn / mapping** (+ Akai profile) — done.
3. ✅ **SQLite track DB + saved cues/loops** — done.
4. ✅ **Headphone/cue monitoring** — done.
5. 🔨 **Stem separation** — model decision made (offline stems · `ort` · htdemucs ·
   optional-download); S1 offline pipeline + resampling done. Next: optional-download, engine
   integration, and per-deck stem UI. See "Decisions made (2026-06-20)".
6. 🔶 **Performance layer** — beat-jump + quantize + loop-roll (slip) + sampler/pads done; more
   beat-synced FX, full slip mode, harmonic-mixing assist, MIDI-mapped pads still open.
7. ⬜ **Typed control bus** — the extensibility enabler (see the deep-dive backlog); unlocks FX
   chains, controller scripting, and a skinnable UI, so it comes before them.
8. ⬜ **Engine/UX deep-dive items** — FX chains + meta-knob, band-RGB waveforms, crossfader curve,
   configurable cue modes, the wider loop toolkit, and sync-coordinator hardening (deep-dive backlog).

---

## Reliability / Pro-Audio Hardening Backlog

These are the next engineering-quality items before the first serious beta release. They are not
flashy features, but they are what make the app behave like reliable DJ software under pressure.

Before implementation, run the research intake in `docs/research/README.md` and
`docs/research/source-intake-queue.md`: read the local architecture first, then the real-time
audio/lock-free sources, then beat-tracking papers, and only then turn the findings into tests,
benchmarks, or small code changes.

1. ✅ **Stronger sync edge-case tests** — added engine tests for paused/empty leaders, unloaded
   followers, sync command cycle-breaking, and loop-roll release while phase-locked; `update_sync`
   now refuses stale sync pulls when either side has no loaded buffer.
2. 🔶 **Device hot-plug and recovery** — master output stream errors now mark audio offline,
   retry the default device on the audio owner thread, and surface online/restarting/error status
   to the footer. Remaining: cue/headphone auto-reopen and state replay after a full stream rebuild.
3. 🔶 **Better underrun/overload telemetry** — title-bar RT load/xrun exists and now separates
   callback over-budget, command-ring full, record-ring drops, and cue-ring drops in the telemetry
   payload/UI tooltip. Remaining: true stream underrun detection and dropped UI telemetry events.
4. ✅ **Booth output** — optional third output stream with independent gain/device selection, fed
   from the post-master mix by default.
5. 🔶 **Master/headphone/record routing model** — mixer taps are now grouped under explicit
   `OutputRouting` buses for record, cue/headphones, and booth. Remaining: user-selectable record
   source/policy and a fuller bus matrix for future mic/aux/stems routing.
6. 🔶 **Latency compensation** — latency-aware play-head telemetry exists; cue/headphone and booth
   streams now publish measured device latency plus their prime-buffer latency through
   `engine_status`. Remaining: recording alignment and applying secondary-output offsets to UI/user
   controls where needed.
7. ✅ **No-drop guarantee for current old buffers** — retired deck/sample `Arc<DeckBuffer>` values
   now go through the reclaim ring or bounded RT-side parking when the ring is full; tests force
   deck and sampler replacement under reclaim pressure. Remaining future work: apply the same
   retire model to large graph/stem snapshot swaps.
8. 🔶 **Controller mapping profiles** — sampler pads are now registered control-bus targets, and
   the bundled Akai MPK Mini MK3 / LPD8 starter profiles map factory pad notes to sampler triggers.
   Continue adding tested profiles (DDJ-SB3, Numark, Hercules, more Akai/Korg), plus hot-plug
   profile activation and per-device HID button/LED support.
9. 🔶 **Modular per-deck processing graph** — `docs/DECK-GRAPH.md` now defines the target stage
   contract, ownership model, and migration plan for
   `source -> playhead/resampler -> keylock -> pregain/ReplayGain -> EQ/filter -> FX -> fader -> buses`.
   Remaining: move `DeckPlayer::next_frame` into those stage structs without changing behavior.

### Research-backed implementation queue

1. ✅ **RT-audio paper/talk notes** — Bencina/Doumler summary and callback-safety audit added in
   `docs/research/`; Doumler is downloaded locally, while direct Bencina re-fetch remains before
   detailed quotation.
2. ✅ **Lock-free/state-handoff design note** — documented immutable graph snapshots, SPSC command
   flow, telemetry counters, and control-thread reclamation for retired buffers/processors in
   `docs/research/lock-free-state-handoff.md`.
3. ✅ **Beat-tracking literature notes** — documented citation status in
   `docs/research/summaries/beat-tracking-literature.md`: Dixon/BeatRoot trail partially verified,
   Laroche and the requested 2024 zero-latency citation still unverified, OBTAIN (2017) downloaded
   as the online fallback, and Beat This (2024) summarized as a modern evaluation/continuity source.
4. ✅ **Beat-tracking benchmark harness** — added `compas-dsp` synthetic regression tests for
   common dance tempos, delayed beatgrid phase, and sparse intros, plus ignored reference cases for
   tempo ramps, half/double-tempo traps, and swung drums. Criterion now also tracks beatgrid cost.
5. ✅ **Adopt one algorithmic improvement at a time** — adoption gate documented in
   `docs/research/beat-tracking-adoption-plan.md`; next algorithm slice is candidate tempo
   diagnostics, then confidence calibration, each with tests, cost check, UI contract, and rollback.

### Research-backed implementation TODO

These are the concrete code/design tasks that came out of the research pass. Work them in order
unless a release-critical bug interrupts; each item should land as its own small commit with tests
or docs updated in the same patch.

1. ✅ **Candidate tempo diagnostics** — `compas-dsp::analysis::estimate_tempo_diagnostics` exposes
   ranked autocorrelation candidates (raw + folded BPM, peak-relative score), half/double-octave
   onset support, the selected BPM, and the selected beat phase. Additive only: `estimate_tempo`/
   `estimate_beatgrid` are unchanged (a shared `analyze_tempo` core keeps them in lockstep). The
   half/double trap fixture now has an active diagnostic test asserting the 64 BPM octave's support
   is visible even while the estimator still picks 128.
2. ✅ **Beatgrid confidence calibration** — `TempoEstimate.confidence`/`BeatGrid.confidence` are now
   built from periodic strength (`best_r`, the fraction of onset energy that repeats — collapses for
   noise/silence/weak onsets where peak prominence alone could not), an octave factor (discounts
   half/double ambiguity), and a rival factor (competing in-range tempo). `BeatGrid` additionally
   folds in phase sharpness so an ambiguous downbeat lowers grid trust. Value-only change — no public
   field/IPC/UI change. Calibrated: clean clicks ~0.56-0.62, half/double trap ~0.27, noise ~0.00.
3. ✅ **Beat continuity tests** — the harness now measures phase drift with an offset-invariant
   metric (the spread, max−min, of the index-aligned `true_i − pred_i` offset), which cancels the
   mod-one-beat phase ambiguity and isolates accumulating tempo error / one-off jumps. Active tests:
   the estimator holds phase over a 40 s track (drift ≈ 0.085 beat) and over a delayed-first-beat
   track, spacing is uniform, and a deliberately 2%-detuned grid is *detected* (proves the metric has
   teeth). No estimator change was needed — its tempo precision already holds phase; these tests lock
   that in against regressions.
4. ⬜ **Expanded beat-tracking benchmark matrix** — promote or add fixtures for half/double traps,
   tempo ramps, swung drums, misleading sparse intros, silence/noise, and a local real-track
   evaluation list kept out of git if audio is copyrighted.
5. ⬜ **Live-input beat-tracking design** — write the design before implementation: chunking,
   no-lookahead timing, latency, routing, clock-domain ownership, and why OBTAIN-style online
   tracking is separate from offline local-file analysis.
6. ⬜ **Modular deck graph refactor** — incrementally move `DeckPlayer::next_frame` into the
   documented stages: source, playhead/resampler, keylock, pregain/ReplayGain, EQ/filter, FX,
   fader, and buses.
7. ⬜ **No-drop graph/stem snapshot retirement** — route future large graph snapshots, stem buffers,
   and model state through the same reclaim/parking model now used for old `Arc<DeckBuffer>` values.

---

## Feature-parity targets (vs Serato / rekordbox / Traktor / mature open-source)

Status: ✅ have · 🔶 partial · ⬜ planned. The controller list lives in `docs/CONTROLLERS.md`.

- ✅ BPM + key detection · ✅ tempo+phase sync (+ tempo-only/leader) · ✅ key-lock + pitch control
- ✅ 4 decks · ✅ EQ + crossfader (curve/additive/reverse) · ✅ beat looping + loop scale/move
- ✅ hotcues · ✅ configurable cue modes · ✅ quantize · ✅ loop-roll · ✅ beat-jump · ✅ ReplayGain
- ✅ **programmable mapping engine** (control bus + declarative bindings + soft-takeover + JS scripting)
- ✅ **DJ controller support** (engine + loader + scripting host wiring); 🔶 **MIDI** profiles to author,
  ⬜ **HID** input layer (`hidapi`) for jog/displays
- 🔨 **effects** (echo/reverb/flanger/bitcrusher in a reorderable chain + meta-knob; ⬜ phaser, FX units UI)
- 🔨 **stem separation** (S1 offline pipeline + resampling done; optional-download + engine/UI pending)
- ✅ master recording · ⬜ **microphone & aux inputs** · ⬜ controller **LED/output feedback**
- ✅ library: crates/playlists, search/sort, BPM/key; 🔶 AutoDJ (planner + queue done; ⬜ unattended chaining)
- ⬜ **vinyl / timecode control** (DVS) · ⬜ reverse / censor
- ⬜ **external library import** — your local files ✅; ⬜ import from other DJ apps + media libraries
- 🔶 **broad format support** — symphonia covers WAV/AIFF/FLAC/MP3/OGG-Vorbis/Opus; ⬜ AAC/MP4 via the
  documented FFmpeg fallback (codec-patent note in the dependency table)
- ⬜ harmonic-mixing assist UI (planner exists) · ⬜ smart crates / tags / folder-watch

## Infrastructure & distribution (pending)

- 🔨 **Release pipeline** (`.github/workflows/release.yml`, via `tauri-action`): on a `v*` tag,
  build **Windows `.msi`/NSIS**, **macOS `.dmg`**, and **Linux** artifacts and publish them as
  GitHub Release assets. Feeds the website download buttons. *(Scaffolded; code signing is a
  follow-up.)*
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

## Decisions made (2026-06-20) — stem separation

- **Mode: offline pre-computed stems, not true real-time.** On load/on-demand, run the model
  once on the existing analysis worker thread and produce 4 stem buffers (drums/bass/other/vocals);
  a deck then holds `[Arc<DeckBuffer>; 4]` and the audio callback mixes 4 frames with 4 per-stem
  gains. Zero model inference on the RT thread — fits the in-RAM play-head model and adds no new
  RT hazard. Live stem control = per-stem gain/mute. True real-time (causal in-path) separation is
  **deferred indefinitely** (GPU/latency cost, worse quality-per-effort).
- **Runtime: `ort` (ONNX Runtime).** A quarantined native dependency, justified the same way as
  the FFmpeg fallback: pure-Rust everywhere except one well-contained, well-supported native lib.
  Chosen over pure-Rust `candle`/`tract` for best model fidelity (runs htdemucs as-is) and the only
  path to DirectML/CoreML/CUDA acceleration (Demucs on plain CPU is ~slower than realtime). Lives in
  a dedicated **`compas-stems`** crate so the native dep stays out of the core engine crates and CI.
- **Model: htdemucs** (Meta, MIT) — hybrid-transformer 4-stem, state-of-the-art quality.
- **Distribution: optional-download.** Installer ships without the ~hundreds-of-MB model + the
  onnxruntime lib; both are fetched (checksum-verified) into the app-data dir on first stem use.
  Keeps the installer lean and makes stems opt-in. (Bundle/defer rejected.)
- **Licensing: clear.** htdemucs, ONNX Runtime all MIT/Apache — no blocker (unlike streaming).

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
