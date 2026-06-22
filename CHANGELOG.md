# Changelog

All notable changes to compas are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); the project uses [SemVer](https://semver.org/).

## [Unreleased]

### Fixed
- **Title-bar update check:** before the first published release feed exists, clicking the version
  chip now reports "up to date" instead of surfacing a raw JSON/404 parsing error.
- **Library loads onto all four decks:** track rows now have **A / B / C / D** load buttons
  (deck-coloured) instead of just A/B, so decks C and D are reachable from the list; the
  loaded-as tag recognises all four.
- **Sync stale-buffer guard:** the audio sync PLL now refuses to apply a stale sync tempo when the
  leader or follower no longer has a loaded deck buffer.
- **Master-output recovery foundation:** CPAL stream errors now mark audio offline, trigger default
  output retry on the audio owner thread, block recording while offline, and surface
  OK/restarting/error status in the footer.

### Changed
- **Calibrated beatgrid confidence (`compas-dsp`):** `TempoEstimate.confidence` /
  `BeatGrid.confidence` now reflect ambiguity honestly. They combine periodic strength (the fraction
  of onset energy that actually repeats — so noise, silence, and weak onsets read as untrustworthy,
  which the old peak-prominence measure could not detect), a half/double octave discount, and a
  competing-tempo discount; the beatgrid value additionally folds in downbeat-phase sharpness. This
  is a value-only change (no public field, IPC, or UI change). Clean clicks land ~0.56-0.62, the
  half/double trap ~0.27, and noise ~0.00.

### Added
- **Candidate tempo diagnostics (`compas-dsp`):** a new `estimate_tempo_diagnostics` exposes the
  ranked autocorrelation candidates, the selected BPM/beat phase, and half/double-octave onset
  support so ambiguous beatgrid decisions are visible before tempo-selection behavior changes. This
  is additive — `estimate_tempo` / `estimate_beatgrid` output is unchanged (the shared
  `analyze_tempo` core guarantees they cannot disagree).
- **App-shell controls wired:** the left rail now performs real actions (Perform scroll, Library
  focus, Crates/source focus, FX focus, Rec toggle). The title bar now has working recording,
  metronome, settings, contrast, and profile controls instead of inert placeholders.
- **AutoDJ queue:** library rows can enqueue tracks; the queue banner can load the next track to an
  empty deck (or deck B fallback) and clear the queue.
- **Controller-panel MIDI connection:** the controller mapping panel can rescan/connect/disconnect
  MIDI devices directly and shows the latest MIDI/HID input while learning mappings.
- **Stem S1 resampling:** `compas-stems` now accepts non-44.1 kHz input by resampling into the
  htdemucs model rate and converting stems back to the source rate; ONNX Runtime is feature-gated
  so default tests do not require a locally installed runtime.
- **Linux release artifacts:** the release workflow now includes an Ubuntu build with Tauri Linux
  system dependencies, so tagged releases can attach Linux artifacts alongside Windows and macOS.
- **Website/app polish:** refreshed the app screenshot, added the mobile hamburger menu, corrected
  four-deck website copy, and replaced the old "Phase 1" footer text with public-beta wording.
- **Architecture hardening plan:** documented the target per-deck processing graph and the
  pro-audio reliability backlog: sync edge-case tests, device recovery, routing, latency, buffer
  reclamation, and controller profile coverage.
- **Research intake wiki:** added `docs/research/` for real-time audio, lock-free state handoff,
  and beat-tracking papers/talks, with a summary-first process before algorithm or architecture
  changes.
- **RT-audio research notes:** added the first Bencina/Doumler summary and a concrete compas
  callback-safety audit checklist covering reclaim, telemetry, rings, and callback boundaries.
- **Lock-free state handoff note:** documented compas's command, buffer, reclaim, telemetry,
  recording, and cue handoff paths plus the next reclaim/diagnostics hardening slice.
- **Beat-tracking literature note:** recorded verification status for Dixon/BeatRoot, Laroche, the
  requested 2024 zero-latency citation, and OBTAIN as the verified online-beat fallback.
- **Research source intake queue:** added the committed read-order/implementation gate for local
  architecture, real-time audio, lock-free state handoff, beat tracking, OBTAIN, and Beat This
  sources. Downloaded papers stay ignored under `docs/research/papers/`.
- **OBTAIN / Beat This summaries:** added implementation-facing notes for OBTAIN (online
  beat-tracking fallback) and Beat This 2024 (accuracy-vs-continuity evaluation warning) before any
  estimator rewrite.
- **Beat-tracking harness:** added public-API synthetic tests for common dance tempos, delayed
  beatgrid phase, and sparse intros, plus ignored reference cases for tempo ramps, half/double
  traps, and swung drums; Criterion now benchmarks `estimate_beatgrid_12s`.
- **Beat-tracking adoption gate:** documented the required source note, target behavior, tests,
  benchmark cost, UI contract, real-time boundary, and rollback path for each future estimator
  change.
- **Sync edge-case tests:** added audio-engine coverage for paused/empty leaders, unloaded
  followers, command cycle-breaking, and loop-roll release while a follower is phase-locked.
- **Split realtime drop counters:** engine load telemetry now reports command-ring-full drops,
  master-record ring drops, and cue/headphone ring drops separately from callback overruns; the
  title-bar RT tooltip shows each counter for faster diagnosis.
- **Booth output:** added an optional post-master booth monitor path with independent output device
  selection and BOOTH level. The mixer pushes the booth tap through a lock-free ring to a dedicated
  CPAL output thread; frontend controls sit under the crossfader next to headphone cue.
- **Explicit output routing model:** grouped record, cue/headphone, and booth sinks under
  `OutputRouting` in the audio mixer, giving future bus/routing policy work one clear owner.
- **Secondary-output latency telemetry:** cue/headphone and booth output streams now publish
  measured CPAL device latency plus their prime-buffer latency through `engine_status`; the footer
  tooltip exposes those numbers for alignment/debugging.
- **No-drop buffer reclaim under pressure:** retired deck/sample `Arc<DeckBuffer>` values now use
  the reclaim ring first, then bounded RT-side parking if the ring is full. Reclaim pressure is
  exposed in engine-load telemetry and covered by deck/sampler replacement tests.
- **Sampler-capable controller profiles:** `sampler.gain` and `sampler.N.trigger` are now registered
  control-bus targets, and the bundled Akai MPK Mini MK3 / LPD8 profiles route factory pad notes to
  sampler pads through the controller engine.
- **Deck graph design:** added `docs/DECK-GRAPH.md`, defining the target per-deck processing stages,
  RT ownership rules, no-drop snapshot retirement model, and migration plan from the current inline
  `DeckPlayer::next_frame` path.
- **Bitcrusher FX:** a new per-deck **CRUSH** insert — lo-fi crunch from bit-depth reduction
  (quantising to as few as ~2 bits) plus sample-rate reduction (a sample-and-hold decimator),
  with **BITS** and **RATE** knobs. RT-safe `compas-dsp::Bitcrusher` (no allocation), inserted
  after the flanger in the per-deck chain; `set_deck_crusher`. Unit-tested.
- **Flanger FX (beat-synced):** a new per-deck **FLANGE** insert in the deck FX rack — a stereo
  LFO-swept comb (quadrature L/R for width) with feedback. The sweep rate is **beat-synced**
  (1/2/4/8-beat period chips) and a **DEPTH** knob sets sweep width + resonance. RT-safe
  `compas-dsp::Flanger` (pre-allocated), inserted after reverb in the per-deck chain; new
  `set_deck_flanger` command. Unit-tested. (Replaces the old disabled FILTER chip — the filter
  remains the mixer's HPF/LPF knob.)
- **MIDI-mappable sampler pads + headphone CUE:** the MIDI-learn target list now includes the
  8 **sampler pads** (a "Sampler" group) and a per-deck **headphone CUE (PFL)** toggle — so a
  controller's drum pads fire samples and a button can pre-listen a deck. The Akai MPK Mini MK3
  starter profile now maps its 8 pads to the 8 sampler pads (knobs stay on the mixer EQ/filter).
- **Sampler / performance pads:** an 8-pad sampler (pads icon in the title bar). Click an empty
  pad to load a short audio file; press a loaded pad to fire it — one-shots overlap
  (polyphonic), and a per-pad ⟳ makes it a loop that toggles play/stop on press. A global LEVEL
  knob. Playback runs in a new RT-safe `compas-audio::sampler` (fixed 16-voice pool, no
  allocation on the audio thread; replaced buffers reclaimed off-thread) summed onto the master
  bus next to the synth, so it's recordable. New `*_sample*` commands; unit-tested.
- **Performance layer — beat jump, quantize, loop roll:** each deck gains a performance row.
  **Q** quantizes hot-cue jumps and beat-jumps to the beatgrid. **◀4 / 4▶** jump the play-head a
  bar back/forward (grid-aligned). **⅛ / ¼ / ½** are held loop **rolls** with true **slip** — the
  engine keeps a shadow play-head advancing underneath while you roll, so releasing drops back in
  exactly where the track would be, as if the roll never happened. New `SetLoopRoll` command +
  `set_loop_roll`; unit-tested slip catch-up.
- **Headphone / cue monitoring (pre-fader listen):** pre-listen any deck on a second output
  device without touching the master. Each mixer channel has a **CUE** (PFL) button; a
  headphone bar under the crossfader picks the **output device**, toggles cue **ON/OFF**, and
  offers **CUE◁▷MASTER** blend + **PHONES** level knobs. The master mixer sums the cued decks
  into a cue bus, blends it with the master, and pushes it through a ring to a **2nd cpal output
  stream** on its own thread (`compas-audio::cue`) — the play-heads/DSP stay in the one mixer, so
  decks never double-advance. The cue stream primes a small latency buffer and re-primes on
  underrun to ride the two devices' independent clocks. New `*_cue_*` commands; unit-tested cue
  summing.
- **SQLite track database + saved cues/loops:** the library and per-track performance state now
  persist in a real database (`rusqlite`, bundled SQLite, in the app-data dir) instead of
  localStorage. Hot cues, the last loop, the manual beatgrid nudge, and a gain trim are
  written through as you set them and **restored when a track is reloaded** onto a deck.
  Analysis (BPM/key) is cached on load and shown in the library; play count + history are
  recorded on first play. The old localStorage library is migrated in once on first run. New
  `db_*` commands wrap a `db.rs` module (schema + WAL + FK cascade), covered by unit tests.
- **MIDI-learn / control mapping:** bind any controller knob, fader, or pad to a deck or mixer
  control. Open the **MIDI mapping** panel (sliders icon in the title bar), click **LEARN** on a
  target, then move the control — the binding is captured and persisted (localStorage). Continuous
  sources (CCs) scale onto each target's range; pads/notes (and knobs past half-travel) fire
  triggers on the rising edge. Targets cover all four decks (gain, 3-band EQ, filter, tempo,
  play/cue/sync/key-lock, hot cues 1–4, 4/8/16 loops + loop-off, echo, reverb) plus the
  crossfader. Ships a one-click **Akai MPK Mini MK3** starter profile (knobs CC 70–77, pads
  notes 36–43; re-learn to match a specific unit). Engine now emits `midi:note` alongside
  `midi:cc`, and a `set_midi_synth` flag gates note→synth routing so a controller can drive deck
  controls without honking the synth (the instrument panel owns the synth path while open).
- **Local dual-deck engine (Phase 1):** in-RAM decode (symphonia), cubic-interpolated
  fractional play-head (instant seek + varispeed), per-deck 3-band EQ, HPF/LPF filter, gain,
  equal-power crossfader, master, lock-free audio thread with reclaim ring.
- **Analysis:** BPM (spectral-flux onset → autocorrelation), beatgrid (beat phase), and musical
  key (chromagram → Krumhansl–Schmuckler, Camelot).
- **4-deck mixing:** all four decks are now playable — two on-screen deck panels with **A/C** and
  **B/D** switching slots (Traktor-style), and a **4-channel mixer** (volume/EQ/filter per deck)
  with a per-channel **crossfader-assign** switch (A / thru / B). Engine routes each deck to a
  crossfader side; telemetry now covers all four decks.
- **Synth instrument + MIDI input:** a polyphonic synth (4 waveforms, ADSR, 16 voices) on the
  master bus, playable from an **on-screen keyboard**, the **computer keyboard**, or a **MIDI
  controller** (via `midir` — connect a device; its notes play the synth, knobs emit `midi:cc`).
  RT-safe (fixed-voice, no allocation). It's mixed into the master, so it's recordable too.
- **Auto-mix / transitions:** an **AUTO** toggle (auto-transition near track end) and a **MIX**
  button (transition now). A transition cues the incoming deck at its first downbeat, beat-syncs
  it to the live deck, starts it, and runs a 16-beat crossfade with a **bass swap** (hands the low
  end over cleanly), then stops the outgoing deck. Frontend orchestration over the existing
  sync/crossfader/EQ — no audio-thread changes.
- **Continuous beat-sync (tempo + phase):** SYNC is now a toggle that holds a follower deck
  locked to a master in the audio thread — a phase-locked loop rate-matches the beat rate and
  nudges the read rate (±8%, click-free) to null the beat-phase error continuously. Composes with
  key-lock/loops; respects manual grid nudges. `SetDeckSync`/`SetBeatgrid` + engine PLL.
- **Key-lock (master tempo):** change tempo without changing pitch, via a hand-rolled,
  RT-safe WSOLA time-stretcher in `compas-dsp` (overlapping Hann grains + waveform-similarity
  search, reads grains straight from the in-RAM buffer, no allocation on the audio thread —
  ~4% of a core per deck). Per-deck `KEY` toggle; `SetDeckKeylock` / `set_deck_keylock`.
- **Beat loops:** manual IN/OUT + 4/8/16-beat grid-snapped loops (RT-safe play-head wrap), with a
  loop region drawn on the waveform; **hot cues** (set/jump/clear).
- **Jog-wheel scratch:** the platter is a draggable, spinning disc — dragging drives the
  audio-thread read-rate from angular velocity (forward + reverse scrub, hold), independent of
  transport. Engine `SetScratch` command + `deck_scratch` IPC; the disc tracks the hand 1:1.
- **FX rack — echo/delay:** RT-safe stereo `Delay` (pre-allocated ring buffer, fractional
  read with one-pole time-glide for tape-style pitch bend, feedback + wet/dry). Per-deck insert
  (post-EQ) via `SetDeckEcho` / `set_deck_echo`; UI is a beat-synced toggle (¼/½/1/2 beats) with
  a single DEPTH knob. Criterion bench added.
- **FX rack — reverb:** RT-safe Schroeder/Moorer-style `Reverb` (8 parallel damped comb filters → 4
  series allpass diffusers per channel, sample-rate-scaled tunings, all buffers pre-allocated).
  Per-deck insert (post-echo) via `SetDeckReverb` / `set_deck_reverb`; UI is a toggle with SIZE
  and MIX knobs. Criterion bench added.
- **RT-load / underrun meter:** the title-bar indicator now shows real audio-thread load
  (processing time ÷ block budget) and counts real-time-budget overruns (xruns), replacing the
  hardcoded "RT OK".
- **Manual beatgrid-anchor edit:** nudge a deck's beatgrid (±5 ms) from the waveform to line it
  up with the audio; feeds both the grid overlay and beat-loop math.
- **Master recording:** one-click record of the master mix to a 32-bit-float stereo WAV. The
  audio thread taps the post-crossfader master into a lock-free ring; a writer thread streams it
  to disk and finalizes on stop (`start_recording` / `stop_recording`). RT-safe — no allocation
  or file I/O on the audio thread.
- **Local library:** add files (persisted), search, load to deck A/B (double-click / buttons),
  with load-progress feedback.
- **Performance UI:** dual decks, center mixer, scrolling zoom waveforms with beat grid, VU
  metering, library browser, frameless window with custom controls.
- **Spotify (Phase 2a):** Authorization Code + PKCE auth and live catalog search (control-only).
- Brand mark + app icons; landing-page website; CI, contributor docs.

### Changed
- **Tempo −/+ buttons** are now a **persistent fine trim** (±0.1% per click, moves the pitch
  fader) instead of a momentary 3% pitch-bend that gave no visual feedback — momentary bend now
  lives on the jog wheel.

### Notes
- Streaming decks are **control-only** by design — services don't expose decoded audio, so DSP
  is locked on them. True mixing is local-files-only.

[Unreleased]: https://github.com/sergiogallegos/compas/commits/main
