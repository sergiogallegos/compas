# Compás Studio — DAW product plan

> **Status: PLAN ONLY. No DAW code exists yet.** Per YAGNI, we write no Compás Studio code until we
> start its phases deliberately. The focus stays on **Compás DJ**. This document is the north star:
> the feature target, the shared-engine boundary, and the phase order — mirroring how the DJ app was
> built (P0 scaffold → P1 engine → … ). Read `ARCHITECTURE-PRODUCTS.md` first.

## 1. Vision

**Compás Studio** is a digital audio workstation (DAW) — a timeline-based environment for composing,
recording, editing, mixing, and producing music. It is a sibling to Compás DJ, sharing the Compás
Rust audio core but with its own engine and UI. The goal is a focused, modern, cross-platform DAW
that does the **90% that working producers actually use every day**, done well, rather than chasing
every feature of the incumbents.

It is a separate product from Compás DJ — different mental model (a timeline you arrange, not two
decks you perform), different UI, different engine. They share the basement, not the room.

## 2. Reference products & study sources

**Feature target (what "good" looks like):**
- **Ableton Live** — the clip/session model, warping (elastic audio), instant-feedback workflow,
  Max-style devices, and its dual Session/Arrangement views. The benchmark for *performance-oriented*
  production and audio warping.
- **FL Studio** — the pattern/step-sequencer + playlist model, the piano roll (widely considered the
  best), per-channel rack, and a fast pattern-based writing flow.
- **Logic Pro** — deep MIDI editing, scoring, comping/take folders, a large stock
  instrument/effect library, Flex Time/Pitch, and a polished traditional linear-arrangement DAW.

We borrow the **best idea from each**: Live's warping + clip launching, FL's piano roll + patterns,
Logic's comping + MIDI depth.

**Open-source code to study (architecture, not features — we write Rust, they're C/C++):**
- **LMMS** — local clone at `C:\Users\sheco\projects\lmms`. Study: its pattern/song-editor model,
  the instrument/plugin (LADSPA/VST) hosting, the mixer + FX-chain model, and how it schedules MIDI.
  Good reference for a pattern-first, FL-like workflow.
- **Zrythm** — <https://gitlab.zrythm.org/zrythm/zrythm>. Study: its modern engine design (a
  processing graph with automatic latency compensation), CLAP/LV2/VST plugin hosting, the
  region/lane/track model, automation, and its undo/serialization architecture. The closest
  open-source analogue to the engine we want.
- (Cross-checked already for the DJ side: **Mixxx** for real-time audio discipline.)

For each subsystem we tackle, the gate is the same as the DJ app: **a short study note + a compas
impact decision + a test/benchmark plan before implementing** (see `docs/research/`).

## 3. Shared-engine boundary

What Compás Studio **reuses unchanged** from the Compás core (`crates/`):
- **`compas-dsp`** — biquads/EQ, filters, time-stretch (WSOLA), the FX primitives
  (delay/reverb/flanger/bitcrusher), and offline analysis (BPM/beatgrid/key). The DAW's audio
  warping builds on the same stretcher family.
- **`compas-sources`** — file decode/import (symphonia), sample-rate conversion.
- **`compas-core`** — domain types, the typed control bus, declarative controller mapping +
  soft-takeover (so hardware control surfaces work in both products).
- **`compas-script`** — the QuickJS sandbox, reused for user macros / scripted devices.

What Compás Studio **builds new** (a `compas-daw-engine` crate + the `apps/compas-studio` shell):
- A **timeline transport** (bars/beats/ticks, tempo map, time signatures, loop/punch).
- A **track/clip/region model** (audio + MIDI clips on lanes, with takes/comping).
- A **processing graph** (tracks → buses → master) with **plugin delay compensation (PDC)**.
- A **plugin host** (format decision in §5).
- A **piano roll + score/automation editors** (UI).
- Project **save/load** (a real serialized session format).

## 4. Core concepts to model

| Concept | Notes |
|---|---|
| **Transport** | Sample-accurate playhead in bars/beats; tempo map (ramps), time-sig changes; loop, punch-in/out, count-in, metronome. |
| **Tracks** | Audio, MIDI/instrument, bus/group, master, automation. Arbitrary count; folders. |
| **Clips / regions** | Audio regions (with fades, gain, warp markers) and MIDI clips (notes + CCs) placed on a timeline; non-destructive. |
| **Mixer** | Per-track volume/pan, inserts (FX chain), sends → buses, groups, master. Real meters, PDC. |
| **Automation** | Per-parameter lanes (volume, pan, plugin params), breakpoint envelopes, read/write/latch/touch modes. |
| **MIDI / piano roll** | Note editing, quantize, velocity, scales, chord tools; MIDI capture; instrument hosting. |
| **Warping** | Elastic audio: warp markers, beat-detection-driven auto-warp, transient/complex modes (on the WSOLA family in `compas-dsp`). |
| **Comping** | Take folders / lanes; select-best across takes into a comp. |
| **Plugins** | Host third-party instruments/effects + bundled Compás devices. |
| **Session view (optional, later)** | Live-style clip-launch grid for performance/sketching. |

## 5. Hard decisions to make before building

These shape everything; resolve them in P0/P1 with a study note each:
- **Plugin format.** Recommendation: **CLAP first** (modern, open, Rust-friendly via `clack`),
  then **VST3** (market reality, via a Rust binding), and **LV2** only if Linux demand warrants.
  Hosting is the single biggest engine subsystem — study Zrythm's host closely.
- **Engine graph.** A node graph with topological scheduling + **automatic PDC**. Decide
  block-based vs per-sample; almost certainly block-based with a fixed internal buffer, RT-safe
  parameter smoothing (reuse `GainSmoother` patterns).
- **Audio backend.** `cpal` (as the DJ app uses) for portability; evaluate per-platform low-latency
  paths (WASAPI exclusive / CoreAudio / JACK-ALSA) later.
- **Project file format.** A versioned, human-diffable session (likely a directory: a serde JSON/RON
  manifest + referenced media), with a migration story from day one.
- **Time model.** PPQ resolution for MIDI; sample-accurate audio; how tempo maps reconcile the two.
- **Undo/redo.** A command/transaction model from P0 — retrofitting undo into a DAW is misery.

## 6. Phased roadmap

Each phase ships something runnable and is gated by tests/benchmarks, exactly like the DJ app.

- **P0 — Scaffold.** `apps/compas-studio` Tauri shell + `compas-daw-engine` crate skeleton on the
  shared core. Empty timeline UI, transport that runs the playhead, metronome. Project save/load of
  an empty session. Undo/redo command bus in place. CI green.
- **P1 — Audio timeline.** Audio tracks; import/place audio regions; the processing graph
  (track → master) with block scheduling; play/stop/loop; per-track gain/pan/mute/solo; basic
  waveform region rendering; non-destructive fades + region gain. *This is the "it's a DAW" moment.*
- **P2 — Recording & editing.** Record audio to tracks (count-in, punch, loop-record → takes);
  region edit ops (split/trim/move/duplicate/crossfade); snapping to grid; the take-folder/**comping**
  workflow. Latency-compensated recording.
- **P3 — MIDI & instruments.** MIDI tracks + clips; the **piano roll** (note edit, velocity,
  quantize, scale/chord tools); host the bundled Compás synth/sampler as instruments; MIDI capture +
  step input from a controller (reuse the control-bus mapping).
- **P4 — Mixer & automation.** Full mixer view: inserts (FX chains from `compas-dsp`), sends, buses,
  groups, master; metering; **automation lanes** with read/write/latch/touch; **plugin delay
  compensation** across the graph.
- **P5 — Plugins.** CLAP host (instruments + effects), then VST3; plugin sandboxing/scanning; param
  automation of third-party plugins; a basic plugin UI window. (Biggest single phase — may sub-split.)
- **P6 — Warping & advanced audio.** Elastic audio (warp markers, auto-warp via beat detection),
  time-stretch/pitch modes on the WSOLA family; Flex-Pitch-style note editing of monophonic audio.
- **P7 — Session view & performance.** Live-style clip grid, scene launch, follow actions; bridges
  the DAW back toward Compás DJ's performance DNA.
- **P8 — Production polish.** Stock device pack (EQ/comp/reverb/delay/saturation/limiter), groove/
  swing, score/notation export (MusicXML/MIDI), stem/track export, render-in-place/freeze, templates.

**Deliberately deferred / out of scope (for now):** video tracks, surround/Atmos, full notation
engraving, cloud collaboration, a mobile DAW, and an in-app marketplace. Revisit only after P0–P5
prove the engine.

## 7. Risks & honest notes

- **Scope.** A DAW is a multi-year effort; the incumbents have hundreds of person-years. We win by
  staying focused (the daily-use 90%) and reusing the Compás core, not by matching feature checklists.
- **Plugin hosting is the hard part.** It's where most indie DAWs stall. Budget for it; study Zrythm.
- **Don't pollute the DJ engine.** Everything here lives in `compas-daw-engine`/`apps/compas-studio`.
  If a need pushes into the shared core, it must generalize cleanly for *both* products or it doesn't
  belong there.
- **Keep Compás DJ first.** This plan exists so the core stays DAW-ready; it is not a signal to start
  building the DAW. We start P0 only when the maintainer says go.
