# Compás — product-family architecture

Compás is a **family of audio products built on one shared Rust core**. Today that's **Compás DJ**
(the DJ/performance app). A **Compás Studio** DAW is planned. This document defines how the family
is organized so the engine is shared without the two products bleeding into each other.

The guiding principle: **share the engine, not the product.** The expensive, product-agnostic
foundation (DSP, decoding, the control bus, analysis) lives once in `crates/`; each product brings
its own *engine shape* and its own UI in `apps/`.

## Repository layout

```
compas/
├── crates/                     shared core + the DJ engine (all library crates)
│   ├── compas-core/            SHARED  domain types, SourceCapabilities, control bus, mapping
│   ├── compas-dsp/             SHARED  RT-safe DSP primitives + offline analysis (BPM/key/beatgrid)
│   ├── compas-sources/         SHARED  AudioSource abstraction, decode/import
│   ├── compas-script/          SHARED  sandboxed JS scripting runtime (QuickJS)
│   └── compas-audio/           DJ-ONLY the Compás DJ engine (decks, crossfader, cue/booth, sync PLL)
├── apps/
│   ├── compas-dj/              the DJ product (Tauri 2)
│   │   ├── src-tauri/          Rust shell: IPC, audio-thread bridge  → depends on the shared core + compas-audio
│   │   └── frontend/           React UI: decks, mixer, waveforms, library
│   └── compas-studio/          the DAW product (planned — does NOT exist yet)
│       ├── src-tauri/          (future) Rust shell  → depends on the shared core + a new compas-daw-engine
│       └── frontend/           (future) React UI: timeline, piano roll, track mixer, plugin racks
├── website/                    one landing site for the family
└── docs/                       design notes + product/architecture plans
```

## Where the line is drawn

The subtle part is that **not everything in `crates/` is shared.** The DSP, sources, control bus,
and analysis are genuinely product-agnostic — an EQ or a decoder doesn't know whether it's feeding a
DJ deck or a DAW track. But `compas-audio` is **the DJ engine**: its `Mixer` is built around four
decks, a crossfader, cue/booth buses, jog scratch, and a sync PLL. That is a DJ-shaped abstraction,
not a neutral core.

A DAW's engine is a different shape — a timeline transport with bars/beats, an arbitrary number of
tracks, buses and sends, automation lanes, MIDI clips and a piano roll, and a plugin graph. So
**Compás Studio gets its own engine crate** (working name `compas-daw-engine`) that sits on top of
the shared core. It does **not** reuse `compas-audio`.

| Layer | Crate(s) | DJ | DAW |
|---|---|---|---|
| RT-safe DSP primitives, analysis | `compas-dsp` | ✅ reuse | ✅ reuse |
| Decode / sources / import | `compas-sources` | ✅ reuse | ✅ reuse |
| Domain types, control bus, mapping | `compas-core` | ✅ reuse | ✅ reuse |
| Scripting sandbox | `compas-script` | ✅ reuse | ✅ reuse (macros/devices) |
| **Engine** (transport + graph + mix model) | `compas-audio` (DJ) · `compas-daw-engine` (DAW) | DJ-only | DAW-only |
| **UI** (Tauri shell + React) | `apps/compas-dj` · `apps/compas-studio` | DJ-only | DAW-only |

## Dependency rules (keep the core honest)

1. **The shared core never depends on a product.** `compas-dsp`/`sources`/`core`/`script` must not
   know about decks, timelines, tracks, or any product concept. If a "shared" change only makes
   sense for one product, it belongs in that product's engine, not the core.
2. **Products depend down, never sideways.** `apps/compas-dj` → its engine + shared core.
   `apps/compas-studio` → its engine + shared core. The two products **never** depend on each other,
   and the two engines never depend on each other.
3. **Don't distort the DJ engine for hypothetical DAW needs (YAGNI).** The discipline is to keep the
   *core* general, not to pre-build DAW abstractions. We write no DAW code until the plan is real
   and we start its phases — see `COMPAS-STUDIO-PLAN.md`.
4. **RT-safety is a core-wide contract.** Every `process*` on the audio path stays allocation-free,
   lock-free, and panic-free regardless of which product calls it (see `AGENTS.md`/`ARCHITECTURE.md`).

## Adding a product (the recipe)

When Compás Studio (or any future product) starts:

1. Add `apps/<product>/` (a Tauri shell `src-tauri/` + `frontend/`), mirroring `apps/compas-dj/`.
2. Add the product's engine crate under `crates/` (e.g. `compas-daw-engine`) that depends only on
   the shared core.
3. Register both in the workspace `Cargo.toml` `members` (the engine crate may also join
   `default-members` so `cargo test` covers it; the Tauri app stays out of `default-members`).
4. Reuse — don't fork — anything in the shared core. If you're tempted to copy a DSP block, lift the
   generalization into `compas-dsp` instead so both products track one source of truth.

## Why a monorepo

One workspace means one source of truth for the engine: fix a DSP bug once, both products get it. It
keeps the "same repo vs separate repos" decision reversible — because the core is clean library
crates, we can later extract them into their own repo with little pain if the products ever need to
ship on independent cadences. We don't have to decide the final topology now; we only have to keep
the engine separable, which we'd want regardless.

## Branding

The family shares the **Compás** brand and the `compasaudio.com` domain. Products are distinguished
by suffix: **Compás DJ**, **Compás Studio**. Binaries follow the package names (`compas-dj`,
later `compas-studio`); the shared landing site routes to each product.
