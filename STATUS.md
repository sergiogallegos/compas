# compas — status & resume point

> Checkpoint for picking work back up. Last updated: 2026-06-24 (UI/UX feature session — see the
> "Resume here (2026-06-24)" section just below). Earlier 2026-06-23 work: Pioneer-style UI + one-window
> layout, full live beat-tracking arc, mic/aux input, stem URL/checksum verified, and a library
> polish round: OR search, smart crates, tags, watched folders). **Everything below is committed AND
> pushed to `origin/main`** (through `adab4a4`). See `ROADMAP.md` for the full plan + **competitive
> feature backlog**, `CHANGELOG.md` for history, `AGENTS.md` for conventions.
>
> **⚠ STEM SEPARATION REMOVED (2026-06-24).** The Demucs/htdemucs AI stem-separation feature was
> deleted entirely — the upstream Demucs project is archived and we're dropping AI for now. Gone:
> the `compas-stems` crate, the `stems` cargo feature, `ureq`/`sha2`/`ort`/onnxruntime deps, all
> `*_stems`/`*_stem_gain`/model-download IPC, the engine's per-deck stem overlay + `StemStretch`
> shared-grain WSOLA, and the STEMS UI panel. Decks read the mix buffer only. (Dead `.stem-*` CSS
> rules remain in `styles.css` — left for Codex to prune.) See `CHANGELOG.md`.
>
> **✅ PRODUCT-FAMILY RE-ARCHITECTURE DONE (2026-06-24, `e1962e9`).** Split into a product family on
> a shared Rust core. The DJ app moved to **`apps/compas-dj/`** (`src-tauri` + `frontend`) and was
> renamed **Compás DJ** (cargo `compas-dj`, productName/title/wordmark, `<title>`). `crates/` holds
> the **shared core** (`compas-core`/`dsp`/`sources`/`script`) plus the **DJ engine** (`compas-audio`,
> DJ-specific). Workspace member, CI paths, tauri-action `projectPath`, `check-versions`, `.gitignore`
> (`apps/*/…`), and all build/run docs retargeted. Plans written: **`docs/ARCHITECTURE-PRODUCTS.md`**
> (family architecture + dependency rules) and **`docs/COMPAS-STUDIO-PLAN.md`** (the phased DAW
> roadmap vs Ableton/FL/Logic; study LMMS @ `C:\Users\sheco\projects\lmms` + Zrythm). Website +
> README + AGENTS/ARCHITECTURE/CONTRIBUTING rebranded. **Build/run now from `apps/compas-dj/`** (see
> Run/verify below). All checks green from the new layout.
>
> **✅ FIRST CROSS-PLATFORM RELEASE GREEN (v0.1.0, 2026-06-24, `68f87a5`).** The `release.yml`
> pipeline builds + signs on Windows, macOS, and Linux — published to a **draft** GitHub Release
> (Win `.msi`/`.exe`, macOS `.dmg`/`.app`, Linux `.deb`/`.AppImage`, signed `latest.json`). The
> **updater signing key is set** (passwordless minisign `82D09E3B222C0BD2`; `TAURI_SIGNING_PRIVATE_KEY`
> secret). Fixes it took: tauri-action `projectPath: apps/compas-dj`; Linux `libasound2-dev` +
> `libudev-dev` + `pkg-config`; removed empty `APPLE_*` secrets (codesign). **Builds are NOT
> code-signed** (no paid Apple Developer ID / Windows cert yet) → first launch warns: Windows
> SmartScreen "Run anyway"; macOS shows *"damaged"* → fix is `xattr -cr "/Applications/Compás DJ.app"`.
> **Both installs maintainer-verified working from the release: Windows (Run anyway) and macOS M2
> (after xattr).** The v0.1.0 GitHub release is **published** (the maintainer published it manually —
> it is no longer a draft). Code-signing is **deferred for now** (the SmartScreen / `xattr` warnings
> are accepted); re-enable the `APPLE_*` block in `release.yml` if/when it's taken on.
>
> **▶ RESUME POINTER (start here next session):** keep focus on **Compás DJ**; the DAW is plan-only
> (no code until its phases start deliberately). The 2026-06-24 session (below) is **all committed AND
> pushed** to `origin/main` (through `5704639`). **Key Shift + Key Sync are DONE this session.**
> **KEY readout follows the shift is also DONE** (`c2304d0`). **Package / export tools — crate export
> is DONE** (manifest `33a43ae` + audio-bundling zip this session; see the session notes below).
> Good next features (v0.2.0 candidates):
> - **Package / export tools — remaining sub-parts.** Crate export/import (both `.compas-crate.json`
>   manifest and `.compas-crate.zip` with bundled audio) is complete. Remaining: **(a) controller
>   profile packs; (b) diagnostics bundle; (c) backup/restore.** See `ROADMAP.md` Tier 3.
> - **macOS/Windows code-signing** — deferred by the maintainer; pick up only when asked.
>
> **NOT yet exercised live:** Key Shift / Key Sync were committed + all checks green (52 compas-audio
> tests, clippy/fmt, tsc/vite) but the maintainer had not finished an in-app listen at session end —
> verify the pitch shift sounds right (tempo unchanged) and KEY⇄ matches keys on the next run.
> (2) **Deferred polish:**
> ~~live OS file-watch~~ **DONE** (`500b291`); ~~FX/sync internal-clock virtual leader~~ **DONE**
> (`556b88d`); ~~FX beat-sync to the internal clock~~ **DONE**; ~~stems shared-grain key-lock~~
> **REMOVED with the stem feature**; remaining: INT CLK row styling polish (`styles.css`, Codex);
> audio-device-thread items (cue/booth auto-reopen, user-selectable record source) — do when NOT
> mid-test. **Coordination:**
> Codex is iterating the jog-wheel `.platter*` rules in `styles.css` — keep commits scoped to your
> own files and have Codex rebase. Both UI/beat-tracking arcs and the library polish round are done.

## ▶ Resume here (2026-06-24 — UI/UX feature session, committed on local `main`)

A focused UI/UX + library round driven by maintainer feedback in the running app, plus a parallel
Codex task. **All committed on `main`** (Codex's commit reviewed + merged first, then mine on top).
Worked Claude+Codex in parallel via scoped, non-overlapping files (briefs in `docs/codex-tasks/`).

- **Waveform interaction** — the big deck lane is now a **vinyl grab-scrub** (press anchors, drag
  moves the track under the fixed NOW needle; a click without drag still needle-drops), fixing the
  old "jumps before release / can't drag" behavior. Added **wheel + Ctrl/⌘-click zoom** (2–64 s).
- **Drag track → deck** — library rows are draggable onto decks to load. Required disabling Tauri's
  native window drag-drop (`dragDropEnabled: false`) — it was swallowing in-webview HTML5 DnD.
- **Analyze-on-import** — a single-flight background worker fills **BPM + key** for imported tracks
  (on import + at launch), so the library columns populate without loading a track onto a deck.
  Decode-failure writes a 0-BPM sentinel so a row is attempted once. New `db::list_unanalyzed` + test.
- **Library columns + key notation** — dedicated **BPM / KEY** columns (no more overlap with the
  Load buttons) and a **Camelot (8A) ⇄ Musical (C#m)** toggle in Settings (persisted), applied to
  the library and each deck's KEY tile via a shared `formatKey()`.
- **I/O moved to Settings** — device pickers (Codex, **CODEX-A**) plus the mic/booth/internal-clock
  control rows now live in Settings ("Audio devices" + "Input & monitoring"); the mixer keeps only
  the headphone CUE.
- **2/4-deck layout** — a **Decks: 2/4** toggle in Settings (default 2). 4-deck mode is a stacked
  layout (A/C · mixer · B/D) with 4 waveform lanes; decks render **compact** (jog platter + pad/loop/FX
  column hidden — maintainer-approved) and the mixer strips lay out **2×2**. 2-deck mode now shows
  only the 2 visible decks' strips.
- **GAIN vs volume fix** — the GAIN knob and channel fader used to write the *same* engine gain.
  Now **GAIN is an input trim** and the **fader is volume** (engine gain = trim × volume, independent;
  external gain changes fold into volume). Channel knobs bigger (2-column, maintainer-chosen), fader
  shorter.
- **Docs** — `ROADMAP.md` stem-removal drift reconciled (no AI/ML; the 2026-06-20 ONNX/htdemucs
  decision marked REVERSED) and a **Package/export tools** backlog item added.

- **Key Shift + Key Sync (v0.2.0 headline)** — per-deck **KEY ±** semitone stepper transposes pitch
  *without changing tempo*, reusing the WSOLA key-lock stretcher: `KeylockStage` gained a `pitch`
  factor (`2^(semitones/12)`) that scales the grain read step while the play-head keeps advancing at
  the tempo rate (a non-zero shift engages the stretcher like key-lock; resets to 0 on load/unload).
  `AudioCommand::SetDeckPitchShift` + the full IPC chain + `useDeck` state/action. **KEY⇄ (Key Sync)**
  picks the nearest ±6-semitone shift to harmonically match the row-partner deck (off the detected
  key via `pitchClassOf`, accounting for the partner's own shift). RT-safe; 1 new engine test
  (`pitch_shift_engages_stretch_and_scales_grain_step`), 52 compas-audio tests pass.
- **KEY readout follows the shift (Key Shift follow-up).** The deck KEY tile now displays the
  *effective* key for the current `pitchShift` (in the active Camelot/musical notation), with the
  tooltip showing `original → effective (+N)`. Frontend-only/display-only — the engine already
  applies the real pitch shift. New `transposeKey()` in `ipc.ts` plus reverse pitch-class→notation
  tables (`PITCH_NAMES`/`CAMELOT_MAJOR`/`CAMELOT_MINOR`) mirroring `analysis.rs`; resolves pitch
  class + mode (name first, Camelot fallback), transposes mod-12, preserves major/minor. The
  **library KEY column stays on the detected key on purpose** (library tracks aren't loaded/shifted;
  only a deck carries a shift). tsc + vite build clean. *Not yet eyeballed in the running app.*
- **Package / export tools — crate manifest slice (first of the Tier-3 feature).** New
  `apps/compas-dj/src-tauri/src/export.rs`: a pure, serde **`CrateManifest`** (version + app stamp +
  crate name/is_playlist/smart_query + tracks) where each `ManifestTrack` carries the full
  performance payload — analysis (BPM/confidence/first-beat/interval/key), grid offset, gain, tags,
  cues, loops, plus a `file` slot reserved for future audio bundling. `gather_crate()` snapshots a
  crate (resolves smart crates once, recording the query); `apply_manifest()` re-imports into a
  (possibly different) library DB — idempotent `add_track`, then analysis/grid/gain/cues/loops/tags,
  and optionally recreates the crate. Reuses `db::track_state` for cues/loops so the schema stays in
  one place; added a shared `db::open_in_memory()` test helper. IPC `export_crate` (gather → JSON →
  write file) / `import_crate` (read → parse → apply, recreate crate); `ipc.ts` `exportCrate`/
  `importCrate` over the existing save/open dialogs (`.compas-crate.json`); Library rail gained a
  ⤒ per-crate export button + a ⤓ CRATES-header import button. **No audio bundling yet** —
  manifest-only; the importer relinks tracks by their stored path. 4 round-trip tests; 20 compas-dj
  lib tests pass, clippy `-D warnings` + fmt + tsc/vite clean. **Not yet exercised in the running
  app.** Remaining export sub-parts: audio-bundling zip, controller profile packs, diagnostics
  bundle, backup/restore.
- **Package / export — audio-bundling zip (export complete).** `export.rs` gained the packaging
  layer over the `zip` crate (added as a direct dep — already in the lock via tauri-plugin-updater,
  **zero new crates**; **Stored entries**, no deflate backend — audio is already compressed, and it
  sidesteps a flate2/zlib-rs resolution break). `assign_bundle_files()` gives each track a unique
  `audio/<file>` name (basename, deduped); `write_package()`/`read_package()` are generic over the
  byte stream so the round-trip is tested **fully in memory** (no temp files). New IPC
  `export_crate_package` (gather → assign → stream files into the zip) and `import_crate` now
  auto-detects `.zip` (extracts audio to `<app_data>/imported/<crate>/`, relinks each track's
  `path`, applies) vs `.json` (manifest-only, as before). Frontend `exportCrate` now writes a
  `.compas-crate.zip`; `importCrate` accepts `.zip`/`.json`. 3 new zip tests (round-trip, dedup,
  bad-zip-rejected) → **23 compas-dj lib tests**; clippy `-D warnings` + fmt + tsc/vite clean.
  **Not yet exercised in the running app.** *(Note: import loads all audio into memory at once — a
  one-shot action, fine for now; stream-to-disk if huge crates become a problem.)* Remaining export
  sub-parts: controller profile packs, diagnostics bundle, backup/restore.
- **Agent-workflow conventions** (`df43705`, from studying `openclaw/openclaw`): AGENTS.md gained an
  **Agent coordination** section (split parallel work by file, delegate via `docs/codex-tasks/`
  briefs on a branch, lead reviews before merge, no surprise GH writes) + a **Refactoring discipline**
  section; CONTRIBUTING.md gained an **AI-assisted contributions** policy (mark AI PRs, Evidence
  section, self-review first).

**Maintainer-verified in the running app:** scrub, zoom, drag-to-deck, auto-analyzed BPM/key, key
notation flip, the I/O→Settings move, and the 2/4-deck toggle + compact 4-deck layout (2-column
bigger knobs and hidden 4-deck controls were the maintainer's explicit choices). tsc + vite + cargo
clippy/fmt clean throughout.

## ▶ Resume here (latest session — 2026-06-23, pushed to `main`)

**Two workstreams landed, all committed + pushed:**

**A. Pioneer-style UI/UX pass + one-window layout (`4d94b26`).** Frontend + one small engine fix:
- **Backlit LED buttons** — lit states glow at the edge/ring over a dark face instead of full-color
  fills (PLAY/CUE/SYNC, `.chip--on`, active loops). **PLAY/CUE and loop IN/OUT are now circular**
  (round Pioneer buttons); CUE = round amber ring, PLAY = round green ring.
- **Flashing states** — PLAY ready-pulse (loaded+paused), end-of-track amber flash on the platter +
  PLAY (`END_WARNING_SECS = 30`), active-loop pulse. Loops light **amber/orange** (Pioneer), not green.
- **Loop IN feedback** — IN now arms (amber-lit) + flashes OUT + draws a loop-in marker on the
  waveform (was a silent no-op; `loop.armed` added to `useDeck`).
- **Smooth waveform** — rAF-interpolated GPU transform of the SVG `<g>` between the 30 Hz position
  samples (no path re-render), with right-edge overscan. Fixes the steppy scroll.
- **One-window layout (Traktor/Serato-style, no page scroll)** — compacted deck/waveform footprint +
  **2-column channel-knob grid** in the mixer so decks + mixer + library all fit; the track list
  scrolls internally with a clean thin scrollbar (`.tl-body`/`.tracklist` got `min-height: 0`).
- **RT meter fix** — skip the first 3 cold warmup callbacks from the overrun count; the startup
  "callback overrun 1" was a harmless false positive (`mixer.rs`).
- **User-confirmed in the running app:** round backlit CUE/PLAY, one-window fit, library usable.

**B. Beat-tracking sparse-intro weighting — adoption-plan slice 4 DONE (`84e1a55`).** `analyze_tempo`
now applies `apply_density_weight`: scales each onset-envelope sample by a local onset *rate* (moving
average of a saturating `env/(env+mean_env)`), so isolated/loud intro hits can't capture tempo/phase
from a denser groove. Clean tracks scaled by a constant → tempo peak + comb phase unchanged. Teeth
test `beatgrid_resists_loud_sparse_intro` (0.224 s off without it, locks on with it);
`misleading_sparse_124` promoted Reference → Solid; bench unchanged (~5.45 ms). Source note:
`docs/research/summaries/sparse-intro-weighting.md`. **Only offline beat-tracking slice left is #5
(online/live-input) — needs its own design note and pairs with mic/aux inputs.**

**C. Microphone / aux inputs — DONE.** New `compas-audio::input` module (cpal **input** stream on a
dedicated thread, mirrors `cue.rs` inverted): captures mic/line-in, duplicates mono→stereo, pushes
into a ring the mixer drains and sums into the master bus (so it's heard, recorded, and fed to
booth). New `AudioCommand`s `StartAuxInput`/`StopAuxInput`/`SetAuxGain`; the mixer prime-buffers and
rides input/output clock drift like the cue/booth monitors. IPC: `list_input_devices`,
`start_aux_input`/`stop_aux_input`/`set_aux_gain` + `useAux` hook + an **AUX row** in the mixer
(device picker, ON/OFF, AUX level). 2 new RT tests (`aux_input_sums_into_master_with_gain`,
`aux_stays_silent_until_primed`); clippy `-D warnings` (engine + app) + fmt + `tsc`/`vite build`
clean. UI verified rendering live. **Not yet exercised with a real mic from here** (needs a hardware
input + clicking ON). Follow-ups: aux as its own channel strip w/ EQ/FX, dedicated mic PFL/cue, and
it's the capture path for live beat-tracking (slice 5).

**D. Live beat-tracking slice 5 — DESIGN NOTE + CORE DONE.**
- **Design note:** `docs/research/live-input-beat-tracking.md` (causal OBTAIN-style tracker on a
  separate analysis thread off the audio callback, fed by a fan-out of the shipped aux capture;
  publishes a `LiveBeatClock` that plugs into the deck sync PLL as a `SyncSource::Live` virtual
  leader guarded by `locked`; streaming-chunk test plan).
- **Implementation slice 1 — `compas_dsp::LiveTracker` (pure, no engine surface).** Causal:
  incremental spectral-flux onset → sliding-window (8 s) autocorrelation tempo, octave-resolved via
  the shared `tempo_prior` → comb-locked phase re-combed each ~16-hop update and advanced by a
  forward oscillator between updates. Allocation-free after `new` (FFT via `process_with_scratch`);
  `push(&[f32]) -> Option<LiveEstimate{bpm,beat_phase,confidence,locked}>` buffers partial hops so
  chunk boundaries never change the result. 7-test streaming harness
  (`tests/live_beat_tracking_harness.rs`): cold-start lock, faster-tempo, relock-after-step,
  dropout, false-onset robustness, silence-no-lock, and the **no-look-ahead determinism** invariant
  (whole-signal == arbitrary chunks, bit-identical). `cargo bench live_tracker_push_1024` ≈ 19.6 µs
  per 23 ms chunk (~0.08% RT, stream-length-independent); `estimate_tempo_8s` unchanged. Offline
  `estimate_*` + matrix untouched (only made a few consts/`tempo_prior` `pub(crate)`).

- **Implementation slice 2 — engine wiring DONE.** The aux capture now fans each frame into a
  second "analysis" ring (`open_aux_input` takes an optional analysis `Producer`); a dedicated
  non-RT thread (`compas_audio::run_live_analysis`) drains it, downmixes to mono, runs the
  `LiveTracker` at the **input device's** sample rate, and publishes a lock-free
  `LiveBeatClock` (bpm/phase/confidence/locked/active). `start_aux_input` spawns it after learning
  the device rate; it self-terminates when capture stops (producer dropped → ring abandoned). New
  IPC `live_beat_clock`; `useAux` polls it ≈8 Hz while capture is on; the **AUX row shows a live
  BPM + a lock dot** (green when locked). 2 new tests (`clock_round_trips…`,
  `run_live_analysis_locks_on_a_click…`). clippy `-D warnings` (engine+app) + fmt + tsc/vite clean.

- **Implementation slice 3 — live-clock deck sync DONE (tempo-match).** The mixer now holds an
  `Option<Arc<LiveBeatClock>>` (shared from the control side via `AudioCommand::SetLiveClock` when
  aux starts); a per-deck `sync_live` flag makes `update_sync` rate-match the deck to the live
  tempo when the clock is **locked** (mutually exclusive with deck-leader sync; an unlocked/absent
  clock holds the deck's tempo). IPC `set_deck_sync_live`; `useDeck` gains `syncLive` +
  `toggleSyncLive`; each deck has a **MIC** chip by SYNC. Test
  `live_sync_tempo_matches_a_locked_clock_and_holds_when_unlocked`. **Deferred (documented):**
  it's mutually exclusive with deck-leader sync.
- **Live phase-lock refinement DONE.** `LiveBeatClock` now timestamps each published phase (shared
  monotonic `epoch` + `stamp_nanos`); `snapshot()` **extrapolates `beat_phase` to now** (advances it
  by the elapsed age at the published tempo), cancelling the analysis/IPC lag across clock domains.
  The mixer's live sync now honors the deck's **sync_mode**: **Full** phase-locks to the live beat
  with the same bounded ±8% bend as deck-to-deck sync; **TempoOnly** just matches BPM. So the deck's
  existing **TEMPO** chip now also toggles live phase-lock vs tempo-only. Tests:
  `live_sync_full_mode_bends_toward_the_live_phase` (+ updated tempo-only/round-trip). Engine-only;
  no IPC/UI change.

**Recommended next:** live stem verification (needs the 301 MB model + `--features stems` build), or
release readiness (updater signing keypair + secrets). The offline beat-tracking adoption-plan queue
(slices 1–5) and the live-input follow-ups are complete.

**E. Library polish round — DONE (all pushed).** (a) **OR search groups** (`c1a4dd1`) —
`artist:daft OR artist:justice`, AND binds tighter than OR, `|` alias. (b) **Smart crates**
(`3574b73`) — save a search as a crate that re-runs its query (✨). (c) **Track tags** (`b67194b`) —
`track_tags` + `tag:` grammar; chips on rows (click-remove) + 🏷 inline add; compose with OR/smart
crates. (d) **Watched folders** (`55a387d`) — auto-import audio from registered folders on add + on
launch (recursive std-fs scan, per-file DB lock, `library:changed` refresh; FOLDERS rail section).
All unit-tested (db tests now 15), clippy/fmt/tsc/vite clean, scoped commits (no `styles.css` →
no collision with Codex's jog-wheel work). **Skipped:** controller profiles (won't fabricate device
MIDI maps). **Remaining polish:** the stem shared-grain key-lock, FX internal-clock virtual leader,
and audio-device-thread items (cue/booth auto-reopen, record-source select) best done when not
mid-test.

**G. Internal-clock virtual leader — DONE (`556b88d`).** A free-running internal master clock
(metronome) the sync engine offers as a virtual leader, so decks have a tempo + phase source with
nothing playing. Engine: `InternalClock { active, bpm, phase }` advanced one frame per `update_sync`
(always "locked"); a per-deck `sync_internal` flag rate/phase-matches it with the same bounded ±bend
as deck-to-deck sync, **mutually exclusive** with deck-leader and live (mic/aux) sync (engaging one
clears the others, both ways). `AudioCommand`s `SetInternalClock`/`SetDeckSyncInternal`; Full mode
phase-locks, TempoOnly matches BPM. IPC `set_internal_clock`/`set_deck_sync_internal`; `useDeck`
gains `syncInternal` + `toggleSyncInternal`; each deck has an **INT** chip by SYNC/MIC; a
`useInternalClock` hook drives an **INT CLK** row in the mixer (ON/OFF + BPM input). 3 new tests
(tempo-match + hold-when-inactive, Full-mode phase bend, mutual-exclusion command path); 55
compas-audio tests pass. clippy `-D warnings` (engine+app) + fmt + tsc/vite clean. **Not yet
exercised live** (needs a running app).
- **FX beat-sync to the internal clock — DONE.** `useDeck` now takes the internal clock and computes
  an effective `fxBeatSec`: when a deck follows the clock (INT), the engine rate-matches its audio to
  the clock tempo, so its **echo/flanger beat-times derive from `60/clockBpm`** instead of the deck's
  analyzed grid (decks not on INT keep using their own grid — musically correct, since the delay/LFO
  must match the audio's actual tempo). A re-push effect re-applies active echo/flanger when the
  effective tempo source changes (live clock-BPM edit, INT toggle, or new grid on load) without a
  chip re-toggle. `pushEcho`/`pushFlanger` extracted to component-scope `useCallback`s; tsc/vite
  clean. Frontend-only. INT CLK row styling still deferred (`styles.css` left to Codex).

**F. Live OS file-watch — DONE (`500b291`).** Watched folders now auto-import on real filesystem
events, not just scan-on-launch/add. Added `notify = "8"`; a `FolderWatch` managed state holds a
live `RecommendedWatcher` (managed unconditionally so the `State` params resolve even if the DB
fails to open). `init_folder_watch` runs at launch (after the launch rescan) and recursively
watches every registered folder; `spawn_folder_watch` debounces event bursts (800 ms quiet gap),
runs the shared `rescan_all_watch_folders`, and emits `library:changed` only when something landed
— exiting cleanly at shutdown when the watcher (and its event sender) drops. `add`/`remove_watch_folder`
now `watch`/`unwatch` the live root. Access events filtered; per-file `track_exists` probe keeps
redundant scans cheap. No frontend change (Tauri injects `State`). `cargo check` + clippy
`--all-targets -D warnings` + `fmt --check` clean; no `unwrap`/`expect`/`panic`. **Not yet
exercised live** (needs a running `tauri dev` to drop a file in and watch it import).

## ▶ Previous session (deck-graph refactor + local-only UI — pushed to `main`)

**1. Modular per-deck graph refactor — DONE (`docs/DECK-GRAPH.md`).** All stages extracted,
behavior-preserving, each its own commit, tests green after each:
- **`ToneStage`** (`a23e644`) — DJ filter → 3-band EQ (`process`/`set_eq`/`set_filter`).
- **`KeylockStage`** (`9d22d78`) — key-lock toggle + WSOLA mix/stem stretchers + the `engaged`
  re-prime flag (`begin_frame`/`mark_jumped`/`set_active`); the ~10 ad-hoc `stretch_engaged = false`
  sites now name the intent.
- **`FaderStage`** (`359e42b`) — channel gain + ReplayGain (`advance`/`apply`). **`FxChain` already
  serves as `DeckFxStage`.**
- **Source-read + play-head advance** (`601f42c`) — extracted as the `read_source_frame` /
  `advance_playhead` methods (kept as methods, *not* a struct: the play-head/loop/sync fields are
  touched ~100× across the sync PLL + telemetry + command handlers, and the PLL needs simultaneous
  `&mut` to two decks — rationale in `DECK-GRAPH.md`). `next_frame` is now a clean pipeline:
  `fader.advance → early-outs → read_source_frame → advance_playhead → tone → fx → fader.apply`.
- 8 isolated stage tests added; **45 compas-audio tests pass**, clippy `--all-targets -D warnings`,
  fmt, and Tauri `cargo check` all clean. (rust-analyzer shows 2 false-positive E0308s in the test
  module — `[f32]` arrays it guesses as `f64`; the compiler is happy.)
- **Pregain/fader split — DONE** (`d962b03`): `PregainStage` (ReplayGain) now applies *before*
  tone/FX; `FaderStage` is channel-gain-only after FX. Intentional behavior change — the nonlinear
  bitcrusher now sees a loudness-normalized input (linear FX unaffected; ReplayGain defaults to 1.0).
  A listening test on loud/quiet tracks with the crusher engaged is still advisable. **This was the
  last open refactor item — the modular deck graph is now complete.**

**2. PLAY-at-end auto-rewind fix** (`1df4bf1`) — pressing PLAY while the play-head is parked at/past
the end now rewinds to the cue point (was a no-op). New test `play_at_end_rewinds_to_the_cue_point`.

**3. Local-only, Traktor-style UI:**
- **Local-only library** (`bf1349b`) — removed the Spotify/Apple/SoundCloud source rows; only Local
  Library remains.
- **Removed the left nav rail** (`79e04e0`) — all its items were redundant; relocated the
  high-contrast/theme toggle to the title-bar top-right. Single-column Traktor-style layout.
  **User reviewed the new layout in the running app and approved it.**
- **Deleted the parked Spotify code** (`chore` commit, −820 lines) — `useSpotify.ts`, `lib/spotify.ts`,
  `src/spotify.rs`, the `mod spotify`/`spotify_listen` registration, and the Spotify-only opener
  plugin/capability/Cargo+npm deps (pruned a large `async-*` tree from `Cargo.lock`). Left the
  `compas-core`/`compas-sources` capability model + `track.ts` `MusicProvider` intact (architecture,
  not the connect flow).

**Recommended next:** push this session to `main` when ready (nothing pushed yet); then a roadmap
front — live stem verification (needs the 301 MB model), beat-tracking slice #4 (sparse-intro
weighting), microphone/aux inputs, or release readiness (updater signing keypair + secrets).

## ▶ Earlier session (2026-06-22 — stems + beat-tracking, pushed to `main`)

**Two workstreams landed this session, all committed + pushed to `main`:**

1. **Beat-tracking adoption-plan slices (offline DSP).** Worked the gated queue in order, one small
   reversible commit each: ① candidate tempo **diagnostics** (`estimate_tempo_diagnostics`) → ②
   **confidence calibration** (honest periodic-strength × octave × rival, so noise/silence read ~0)
   → ③ **continuity tests** (offset-invariant phase-drift metric) → ④ **evaluation matrix** +
   git-ignored **real-track eval** harness → ⑤ first real algorithm change: **octave-aware
   half/double scoring** (`select_tempo` + dance-tempo prior). Details below under "Research point"
   / "Beat-tracking TODO" entries. Adoption plan: `docs/research/beat-tracking-adoption-plan.md`.

2. **Stem separation — now functionally complete (the marquee feature).** Six commits:
   **S2 engine core** (per-deck `Option<[Arc<DeckBuffer>;4]>` + smoothed gains + per-stem WSOLA,
   RT-safe no-drop retirement) → **S2 IPC + worker job** (`separate_stems`, gated behind a `stems`
   cargo feature so the default build links no onnxruntime) → **S3 STEMS UI** (Separate + progress,
   DRUMS/BASS/OTHER/VOX knobs + mute, revert) → **disk cache** (feature-independent reload) →
   **in-app model download** (GET MODEL button). See the stem-separation section below for the full
   record. **Two honest caveats:** (a) the default htdemucs model URL/checksum are *flagged
   placeholders* — verify + bake a sha256 before the first stem-enabled release; (b) nothing in the
   stem path has been run **live** from here (no 301 MB model / `--features stems` build / network) —
   all code compiles, lints (`-D warnings`, both feature configs), type-checks, and is unit-tested,
   but an actual separation/download run is unverified.

**Recommended next (pick one):**
- **Live stem verification:** on a machine with the model, build `--features stems`, run a real
  separation + the GET MODEL download, and confirm the URL/checksum (then bake the sha256).
- **Pro-audio hardening backlog** (`ROADMAP.md`): the **modular per-deck graph refactor** (move
  `DeckPlayer::next_frame` into staged structs) is the documented next hardening step and would make
  future stem/FX routing cleaner.
- **Other roadmap fronts:** microphone & aux inputs, controller LED/output polish, or release
  readiness (updater signing keypair + secrets).

---

## ▶ Earlier resume notes (pre-2026-06-22 session)

**Seven-item polish/import batch (2026-06-22) — done.** Ported the high-value process and
engineering practices from `rust-ethernet-ip`, then used the same pass to close real app polish:
- **UI controls wired:** left nav items now focus/open real parts of the app, the title-bar REC
  button controls master recording, the metronome clicks at master BPM, settings/profile panels
  open, and the bottom status no longer says "Phase 1".
- **Screenshots refreshed:** README and website hero screenshots now show the current UI with the
  overview waveform and public-beta footer.
- **Stems advanced:** `compas-stems` now resamples non-44.1 kHz sources into the htdemucs model
  rate and back to source rate; the ONNX Runtime dependency is feature-gated so normal CI can test
  the crate without a locally installed runtime.
- **Controller panel improved:** the controller mapping panel can connect/rescan/disconnect MIDI
  devices itself and shows the latest MIDI/HID input while learning.
- **Library AutoDJ queue:** track rows can enqueue tracks; the queue banner can load the next item
  to an empty deck (or deck B fallback) and clear the queue.
- **Release readiness:** the tag workflow now builds Windows, macOS, and Linux artifacts.
- **Docs/status cleanup:** roadmap/status/changelog/website docs now match the above.

**Next recommended workstream:** pro-audio hardening before release. Track it in `ROADMAP.md`
under "Reliability / Pro-Audio Hardening Backlog": no-drop tests for retired
`Arc<DeckBuffer>`/graph state, more controller mapping profiles, and the modular per-deck
processing graph
`source -> playhead/resampler -> keylock -> pregain/ReplayGain -> EQ/filter -> FX -> fader -> buses`.

**Hardening item 1 done:** sync edge-case coverage now exercises paused/empty leaders, unloaded
followers, sync command cycle-breaking, and loop-roll release while phase-locked. The audio-thread
sync PLL now also requires both leader and follower buffers to be loaded before applying a sync
tempo. Verified with `cargo test -p compas-audio --locked` and
`cargo clippy -p compas-audio --all-targets -- -D warnings`.

**Hardening item 2 partial:** the master output stream now records CPAL stream errors, marks audio
offline/restarting in shared status, and retries the default device from the audio owner thread.
The footer polls `engine_status` and shows OK/restarting/offline plus the latest error. Recording is
blocked while the master output is offline. Remaining follow-up: cue/headphone auto-reopen and state
replay after a full stream rebuild. Verified with `cargo check -p compas-audio --locked`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and
`cd frontend && npx tsc --noEmit`.

**Hardening item 3 partial:** realtime telemetry now separates callback overruns, command-ring-full
drops, master-record ring drops, and cue/headphone ring drops. The title-bar RT tooltip exposes each
counter instead of collapsing every problem into one xrun number. Remaining follow-up: true hardware
stream underrun detection and dropped UI telemetry-event accounting. Verified with
`cargo test -p compas-audio --locked`, `cargo clippy -p compas-audio --all-targets -- -D warnings`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and
`cd frontend && npx tsc --noEmit`.

**Hardening item 4 done:** booth output is now an optional third output stream fed from the
post-master mix. The mixer owns an independent smoothed booth gain and pushes the booth tap through
a lock-free ring to a dedicated CPAL output thread; the UI exposes device selection, ON/OFF, and
BOOTH level. Verified with `cargo test -p compas-audio --locked`,
`cargo clippy -p compas-audio --all-targets -- -D warnings`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and
`cd frontend && npx tsc --noEmit`.

**Hardening item 5 partial:** the audio mixer now groups record, cue/headphone, and booth taps
under an explicit `OutputRouting` model instead of scattered sink fields. This gives the future
bus matrix a clean landing point without changing behavior. Remaining follow-up: user-selectable
record source/policy and routing choices for future mic/aux/stems buses. Verified with
`cargo test -p compas-audio --locked` and
`cargo clippy -p compas-audio --all-targets -- -D warnings`.

**Hardening item 6 partial:** secondary output streams now publish latency telemetry. Cue/headphone
and booth output threads write measured CPAL device latency plus the known prime-buffer latency to
atomic `MonitorLatency` probes, `engine_status` exposes those values, and the footer tooltip shows
them. Remaining follow-up: align recordings and apply secondary-output offsets where user-facing
controls need them. Verified with `cargo test -p compas-audio --locked`,
`cargo clippy -p compas-audio --all-targets -- -D warnings`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and
`cd frontend && npx tsc --noEmit`.

**Hardening item 7 done for current buffers:** retired deck/sample `Arc<DeckBuffer>` values now
use the reclaim ring first, then a fixed-size RT-side parking buffer if the ring is full. The
title-bar RT tooltip exposes reclaim pressure, and tests prove deck and sampler replacement do not
drop the old buffer on the callback path while reclaim is full. Remaining follow-up: route future
large graph/stem snapshots through the same retire model. Verified with
`cargo test -p compas-audio --locked`, `cargo clippy -p compas-audio --all-targets -- -D warnings`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and
`cd frontend && npx tsc --noEmit`.

**Hardening item 8 partial:** sampler pads are now first-class control-bus targets
(`sampler.N.trigger` plus `sampler.gain`), and the bundled Akai MPK Mini MK3 / LPD8 profiles route
their factory pad notes to sampler pads instead of the stale cue/sync bindings. Bundled profile
validation continues to assert every binding targets a real registered control. Remaining follow-up:
more hardware-authored profiles (DDJ-SB3, Numark, Hercules) and per-device HID/LED behavior.

**Hardening item 9 design slice:** `docs/DECK-GRAPH.md` now defines the modular per-deck graph
contract: source -> playhead/resampler -> keylock -> pregain/ReplayGain -> EQ/filter -> FX ->
fader -> buses. It also documents ownership, no-drop graph-snapshot retirement, current-code
mapping, and the migration/test plan. Remaining follow-up: refactor `DeckPlayer::next_frame` into
those stage structs incrementally.

**Research intake added:** before starting those hardening tasks, use `docs/research/README.md` and
`docs/research/source-intake-queue.md`. Read order is: local architecture/RT rules →
Bencina/Doumler real-time audio + lock-free/state handoff → Dixon/Laroche beat-tracking trail →
verified online/zero-latency source or OBTAIN fallback → modern evaluation notes. Do not implement
directly from a paper until there is a short summary, a compas impact decision, and a
test/benchmark plan.

**Research point 1 done:** Bencina/Doumler real-time audio notes live in
`docs/research/summaries/rt-audio-bencina-doumler.md`, and the actionable callback audit lives in
`docs/research/rt-audio-audit.md`. Doumler's lock-free article is downloaded locally; direct
Bencina article re-fetch is still pending before detailed quotation, but the implementation guidance
is enough to continue reclaim/telemetry/deck-graph hardening.

**Research point 2 done:** lock-free/state handoff design lives in
`docs/research/lock-free-state-handoff.md`. It documents current command, buffer, reclaim,
telemetry, recording, and cue handoff paths, then defines the next code slice: split diagnostic
counters, command/reclaim observability, no callback-side large drops, and tests under reclaim
pressure.

**Research point 3 done:** beat-tracking literature verification lives in
`docs/research/summaries/beat-tracking-literature.md`. Dixon is documented through the BeatRoot
trail but the requested exact title still needs a primary source; Laroche (2003) and the requested
Mierer/Mierer-like 2024 zero-latency citation remain unverified; OBTAIN (2017) is downloaded and
summarized as the online beat-tracking fallback; Beat This (2024) is summarized as a modern
accuracy-vs-continuity evaluation source. Decision: build diagnostics, confidence, continuity, and
benchmark coverage before changing algorithms.

**Research point 4 done:** `crates/compas-dsp/tests/beat_tracking_harness.rs` now covers common
dance tempos, delayed first-beat phase, and sparse intros through the public analysis API. Tempo
ramp, half/double-tempo trap, and swung-drum cases are present as ignored reference tests until the
algorithm is ready. `crates/compas-dsp/benches/dsp.rs` also benchmarks `estimate_beatgrid_12s`.
Verified with `cargo test -p compas-dsp --locked`.

**Research point 5 done:** `docs/research/beat-tracking-adoption-plan.md` now defines the gate for
algorithm changes: source note, target behavior, tests first, benchmark cost, UI contract, RT
boundary, and rollback path. The planned order is candidate tempo diagnostics first, then confidence
calibration, before any tempo-selection rewrite.

**Beat-tracking TODO 1 done (candidate tempo diagnostics).** `compas-dsp::analysis` now has a shared
`analyze_tempo` core feeding both the public estimator and a new, additive
`estimate_tempo_diagnostics`. The diagnostics expose ranked autocorrelation candidates (raw + folded
BPM, peak-relative score), the selected BPM/beat phase, and half/double-octave onset support — so the
half/double trap is visible (the estimator picks 128 BPM while the diagnostics show the 64 BPM octave
has ~1.69× the winning peak's support). `estimate_tempo`/`estimate_beatgrid` output is unchanged. New
active harness test `diagnostics_expose_half_double_candidates` plus unit tests asserting the
diagnostics never drift from the public API. Verified with `cargo test -p compas-dsp --locked` and
`cargo clippy -p compas-dsp --all-targets --locked -- -D warnings`.

**Beat-tracking TODO 2 done (confidence calibration).** `TempoAnalysis::confidence()` now combines
periodic strength (a saturating map of `best_r`, the fraction of onset energy that repeats — this is
what finally drops noise/silence/weak-onset tracks to ~0, which the old peak-prominence measure
could not), an octave factor (half/double discount), and a rival factor (competing in-range tempo).
`estimate_beatgrid` additionally folds in downbeat-phase sharpness from the comb. Value-only change:
no public field/IPC/UI change, and `bpm_confidence` is stored but not yet gated on. Measured: clean
clicks ~0.56-0.62, half/double trap ~0.27, noise ~0.00. New tests
`confidence_calibration_orders_clean_above_ambiguous_above_noise` (unit) and
`confidence_is_lower_for_ambiguous_grids` (harness). Verified with `cargo test -p compas-dsp
--locked`, `cargo clippy -p compas-dsp --all-targets --locked -- -D warnings`, and `cargo fmt`.

**Beat-tracking TODO 3 done (beat continuity tests).** The harness now checks phase *continuity*, not
just average BPM, using an offset-invariant drift metric: the spread (max−min) of the index-aligned
`true_i − pred_i` beat offset, which cancels the mod-one-beat phase ambiguity and isolates
accumulating tempo error / one-off jumps. Active tests: the estimator holds phase over a 40 s track
(drift ≈ 0.085 beat) and a delayed-first-beat track, spacing is uniform, and a deliberately
2%-detuned grid is *detected* (the metric has teeth). Honest finding: no estimator change was
needed — its tempo precision already holds phase; these tests lock that in. Verified with
`cargo test -p compas-dsp --locked`, clippy, and fmt.

**Beat-tracking TODO 4 done (evaluation matrix + real-track eval).** `beat_evaluation_matrix` is a
tiered table over clean tempos, delayed phase, sparse intros, silence, noise, half/double traps,
tempo ramps, swung drums, and misleading sparse intros: Solid-tier cases are asserted (regression
guard), Reference-tier known-gaps are printed but not asserted. The report shows the gap is now
narrow — only the half/double trap fails (3/4 reference pass; ramp/swung/misleading-sparse already
pass). A separate `beat_real_track_eval` reads a git-ignored WAV corpus + `manifest.csv` (env
`COMPAS_BEAT_EVAL`; built-in WAV reader, no decoder dep; skips cleanly when unset; relative manifest
paths resolve against the crate dir) and reports exact + within-octave hit rates; proven end to end
against the generated 120/128 BPM test WAVs (2/2 EXACT). See `crates/compas-dsp/eval/README.md`.
Criterion now also benches the tempo estimator across clean/trap/noise. Verified with `cargo test
-p compas-dsp --locked`, `cargo clippy -p compas-dsp --all-targets --locked -- -D warnings`, fmt.

**Half/double tempo scoring done (adoption-plan slice 3 — first real algorithm change).**
`TempoAnalysis::select_tempo` now scores the winning lag and its ½×/2× octaves by
`onset_support × tempo_prior(bpm)` (broad log-normal resonance peaking ~125 BPM, used only to break
2:1 ties) instead of trusting the largest autocorrelation peak. Resolves half/double traps to the
danceable octave; clean 90/120/128/150 unchanged. New active test
`beat_tracking_resolves_half_double_tempo_trap` (de-ignored) plus
`octave_scoring_lifts_accent_trap_to_dance_tempo` (teeth: raw peak 75 → resolved 150); matrix now
has `half_double_trap` + `accent_trap_150` as Solid. Diagnostics share `select_tempo` so
`selected_bpm` stays in lockstep (may now be an octave of `candidates[selected]`). No public
field/IPC/UI change. Cost: `estimate_tempo` ~+4% (5.45 → 5.68 ms on the 8 s bench, offline). Source
note: `docs/research/summaries/half-double-tempo-scoring.md`. Verified with `cargo test -p
compas-dsp --locked`, clippy `--all-targets`, fmt, and `cargo check` on the Tauri app.

**Next beat-tracking task:** the offline beat-tracking slices that were planned (adoption-plan 1-3 +
research TODOs 1-4) are now all done. Remaining adoption-plan slices: **4. sparse-intro weighting**
(reduce isolated intro hits when a later steady region is stronger — `misleading_sparse_124` already
passes, so add a harder variant first) and **5. online/live-input tracking** (needs its own design
note before any code; matches research-backed TODO 5). Per the gate, validate `select_tempo`'s prior
against a real-track corpus (`crates/compas-dsp/eval/`) before trusting it on genuinely slow
material. Or pivot back to the pro-audio hardening backlog (modular deck graph, TODO 6/7).

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
  controller-bus dispatcher + activate/deactivate IPC + the **guided learn editor** (wiggle-to-bind
  over the control registry; MIDI events now carry channel; save/activate profiles) + **bundled
  starter profiles** (Korg nanoKONTROL2, Akai MPK Mini MK3, Akai LPD8 — knobs→gains/filters,
  Akai pads→sampler triggers, from each device's factory-default map; a `controllers::tests` check asserts every
  bundled binding targets a real control) + **output/LED feedback ✅** (design fork resolved:
  reflect *all* changes — the frontend pushes each mapped control's value via `controller_feedback`
  on any UI/controller change; the engine maps engine-value→MIDI through the control behavior and
  sends to every bound address, deduped per address; controller-driven moves still echo immediately;
  a `controller:resync` window event re-syncs the device on profile activation) + **HID input ✅
  (foundation)** (`hidapi`: a `hid` backend module enumerates devices, opens by path, and runs a
  reader thread that diffs input reports and forwards each *changed* byte to the controller engine;
  a new `InputKind::Hid { byte }` resolves at 8-bit scale through the same mapping/soft-takeover
  pipeline, so the learn editor binds HID by wiggling — `hid:input` events feed capture; `hid_list`/
  `hid_connect`/`hid_disconnect` IPC + a device picker in the controller panel). **HID scope:**
  absolute single-byte axes (knobs/faders/jogs); bit-packed buttons and device-specific **output/LED**
  reports are hardware-gated per-device follow-ups. + **DJ-controller starters ✅** (Pioneer DDJ-400
  + DDJ-FLX4, bundled): decks on MIDI ch 1/2, globals on ch 7; channel fader→gain, EQ hi/mid/low,
  COLOR→filter, tempo, PLAY/CUE/SYNC, crossfader, headphone mix — derived clean-room from each
  device's MIDI assignment facts (14-bit faders bound on the MSB for 7-bit control); jog/hotcue/
  loop/pad/FX are unmapped (no control-bus target yet). Also: a hardware PLAY button now latches
  (toggle-on-press) instead of being momentary. **Remaining:** more profiles (DDJ-SB3, Numark,
  Hercules — same method); per-device HID button/LED work.

Other deferred follow-ups (flagged in commits): FX internal-clock virtual leader; library
OR-search/smart-crates/tags/folder-watch; AutoDJ auto-chain/planner UI polish; stem-separation
optional-download/runtime packaging. Brand stays **compas**; domain **compasaudio.com**.


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

**Stem separation (S1)** remains as previously noted (offline pipeline + source-rate resampling
done; optional-download/runtime-packaging follow-ups).

**User-confirmed working in the running app (2026-06-19):** the user ran `tauri dev` and reported
the recent batch works — FX rack (echo/reverb/flanger/bitcrusher), performance row (quantize /
beat-jump / loop-roll), sampler pads, headphone cue, and A/B/C/D library loading. (Hardware MIDI
with the MPK MK3 still needs the controller on hand; the rest is confirmed.)

Most recent feature was the **Bitcrusher FX** (CRUSH chip), preceded by the flanger, MIDI-mapped
sampler pads, the sampler, the perf row, cue monitoring, the SQLite DB, MIDI-learn, and 4-deck —
all described in the **Done** section below. SQLite was also verified end-to-end via a live DB
query (migration + analysis-cache + play history).

**Real hardware MIDI — VERIFIED (2026-06-24).** The maintainer ran **Compás DJ from the new
`apps/compas-dj/` layout** and performed a live **two-song transition** with an **Akai MPK Mini**:
knobs mapped (via MIDI-learn) to a deck's **low/mid/hi EQ + gain**, and **play/stop** on buttons —
all worked. This validates the full session arc in the running app (monorepo move + `compas-dj`
rename + stem-feature removal) with live hardware, and closes the long-standing "MIDI with real
hardware" unverified caveat. (Earlier note kept for history below.)

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
     window, weight-normalized), interleave/deinterleave, and offline sample-rate conversion into
     the model's fixed 44.1 kHz rate with stems converted back to the source rate. Note: the
     single-file htdemucs graph bakes STFT **and** mean/std normalization inside, so the host does
     **no** normalization — just resample + chunk + window + overlap-add. **Verified:** `cargo test`+`clippy` green; the live `ort` smoke test
     (`-- --ignored` with `COMPAS_HTDEMUCS_ONNX=<cached htdemucs.onnx>`) loads the real 301 MB model
     and runs a `[1,2,343980]`→`[1,4,2,343980]` frame in ~4.6 s — **Rust path proven**.
     **Remaining S1 follow-ups:** checksum'd optional-download of the model (HF
     `StemSplitio/htdemucs-onnx` or our own mirror) into the app-data dir; switch `ort` to
     `load-dynamic` so the runtime ships via that download path; consider swapping the lightweight
     linear offline resampler for `rubato` before release.
   - **S2 — engine integration. 🔨 Engine-core DONE (2026-06-22).** `DeckPlayer` now holds
     `stems: Option<[Arc<DeckBuffer>; 4]>` + 4 smoothed `stem_gains` + 4 WSOLA `stem_stretch`. When
     stems are present, `next_frame` reads & sums them at the deck play-head (per-stem key-lock),
     instead of the mix `buffer` (which still drives length/play-head/grid/`base_ratio`). New RT-safe
     `AudioCommand`s `LoadDeckStems`/`ClearDeckStems`/`SetDeckStemGain`; `LoadDeck`/`UnloadDeck`
     retire stems too. Stem-sized retirement goes through the **no-drop reclaim/parking path**, now
     sized for a whole deck (mix + 4 stems) swapping at once (`PENDING_RECLAIM_CAP`,
     `EngineConfig.reclaim_capacity = 48`) — this folds in hardening **TODO 7** for stems.
     `compas-audio` stays free of `compas-stems`/`ort` (stems arrive as `[Arc<DeckBuffer>; 4]`);
     `DeckTelemetry::stems_loaded` exposes state. 4 new tests (sum/mute, clear-reverts,
     load-clears-stems, no-drop stem parking). Verified `cargo test/clippy --all-targets/fmt` on
     `compas-audio` + `cargo check` on the Tauri app.
     **S2 IPC + separation job DONE (2026-06-22).** `separate_stems` (Tauri command) decodes on a
     worker thread, runs htdemucs with `stems:progress` events, and installs the 4 stems via
     `EngineMsg::LoadDeckStems`; `clear_deck_stems`/`set_deck_stem_gain`/`stems_model_status` round it
     out. The native ONNX runtime is behind a new **`stems` cargo feature** (`compas-stems/onnx`), so
     the default build/CI link no onnxruntime (verified: default `cargo clippy -D warnings` clean; and
     `--features stems` `cargo check`+clippy clean too). Model path = `COMPAS_HTDEMUCS_ONNX` or
     `<app-data>/models/htdemucs.onnx`; missing → clear error.
     **Remaining S2:** (1) disk + SQLite cache of the 4 stem WAVs so reload is instant (separation is
     minutes-slow); (2) in-app checksum'd model download into `<app-data>/models/`; then S3 UI.
     **Shared-grain key-lock — DONE (`558fcdf`).** The independent per-stem WSOLA was replaced by
     `compas_dsp::StemStretch<N>`: one similarity search on the mix places each grain for all four
     stems, so inter-stem transients stay phase-coherent (`Σ OLA(stemᵢ,pos) = OLA(Σ stemᵢ,pos)`).
     Cheaper too (one search, not four). 2 DSP teeth tests; all engine tests + clippy/fmt clean.
   - **S3 — UI. ✅ DONE (2026-06-22).** Each local deck has a STEMS control: a separate button (with
     a live progress strip), DRUMS/BASS/OTHER/VOX level knobs with per-stem mute, and a one-click
     revert to the full mix. Disabled w/ explanatory tooltip when the build lacks stem support or the
     model is missing; separation errors surface inline. New `ipc.ts` wrappers
     (`separateStems`/`clearDeckStems`/`setDeckStemGain`/`stemsModelStatus`) + `stems:*` event
     listeners; stem state lives in `useDeck` (resets on track load, since the engine clears stems).
     Verified `npx tsc --noEmit` + `npx vite build` clean.
   - **S2 disk cache + model download DONE (2026-06-22).** Separated stems cache to
     `<app-data>/stems/<key>` as 4 WAVs (key = hash of path+size+mtime); a re-separate loads in
     seconds by decoding them, and the cache-load path is **feature-independent** (a non-`stems`
     build can replay cached stems). The STEMS panel's **GET MODEL** button streams htdemucs into
     `<app-data>/models/` with live progress (atomic `.part`→rename, optional
     `COMPAS_HTDEMUCS_SHA256` check); downloader + `ureq`/`sha2` are gated behind `stems` so the
     default build stays pure. Verified default + `--features stems` `cargo clippy -D warnings` and
     `cargo fmt` clean, plus `tsc`/`vite build`. **FLAGGED for release:** the default model URL +
     checksum are unverified placeholders (like the `TAURI_SIGNING_*` ones) — confirm the real
     `StemSplitio/htdemucs-onnx` URL + bake a sha256 before the first stem-enabled release.
     **Not yet exercised live:** an actual separation/download run needs the 301 MB model + a
     `--features stems` build + network; all code paths compile/type-check but haven't been run end
     to end from here.
2. **More performance layer:** sampler/pads (reuse the synth voices), more + beat-synced FX,
   full global slip mode + reverse/censor, harmonic-mixing assist (we already detect Camelot key).
3. **Release infra — wiring done, secrets pending (2026-06-20).** Auto-update plugin, manual
   "check for updates" on the title-bar version chip, and a git-sha build chip all integrated;
   `release.yml` now builds Windows, macOS, and Linux artifacts and its env block is fully
   populated for `TAURI_SIGNING_*` + Apple notarization. Before
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
- **Booth output** — optional post-master monitor output with independent device selection and
  BOOTH level. The mixer pushes through a ring to a 3rd cpal output stream; UI controls live under
  the crossfader next to headphone cue.
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
(Windows/macOS/Linux installers on `v*` tag; signing commented), `audit.toml`, criterion DSP benches,
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
cargo test -p compas-dsp -p compas-audio                       # engine unit tests
cargo clippy --all-targets -- -D warnings                      # engine lint
cargo check --manifest-path apps/compas-dj/src-tauri/Cargo.toml # the Compás DJ app crate (separate from default-members)
cd apps/compas-dj/frontend && npm install && npx tsc --noEmit && npx vite build
node scripts/make-test-audio.mjs                               # 120/128 BPM test WAVs -> samples/
```
**Launching Compás DJ (Windows, this machine):** `cargo tauri dev` is NOT installed; run the local
Tauri CLI **from the product dir** `apps/compas-dj/` (so it finds its `src-tauri/` sibling, and its
`beforeDevCommand` runs Vite in `frontend/`):
```bash
cd apps/compas-dj && ./frontend/node_modules/.bin/tauri dev
```
If it errors with "Port 5173 already in use", a previous Vite lingered — kill the PID listening on
5173 (and any stray `compas-dj.exe`) first. The legacy-PowerShell-profile `Set-PSReadLineOption`
error that prints on npm/pwsh calls is harmless noise.
