# AGENTS.md — orientation for AI agents & contributors

Read this before making changes. It's the fast path to being productive in compas without breaking
the things that matter.

## What compas is
**Compás** is a product family sharing one Rust audio core. **Compás DJ** (`apps/compas-dj`) is the
real-time professional DJ app and the current focus. **Compás Studio** (a DAW) is planned — see
`docs/COMPAS-STUDIO-PLAN.md` and `docs/ARCHITECTURE-PRODUCTS.md`. **Rust audio core + TypeScript UI
in Tauri 2.** Windows is the primary target; macOS is first-class; Linux best-effort. MIT.

## Repository map
```
crates/compas-core      shared core: types (TrackMetadata, SourceCapabilities, DeckBuffer), control bus, mapping
crates/compas-dsp       shared core: real-time-safe DSP (rt::) + offline analysis (analysis:: BPM/beatgrid/key)
crates/compas-sources   shared core: AudioSource abstraction — LocalFileSource (PcmSource) + decode/import
crates/compas-script    shared core: sandboxed JS controller-scripting runtime (QuickJS)
crates/compas-audio     Compás DJ engine: cpal engine, lock-free rings, Mixer (decks/crossfader/cue/booth), telemetry
apps/compas-dj/src-tauri  Compás DJ Tauri app: IPC commands, audio-thread bridge
apps/compas-dj/frontend   Compás DJ UI: React + Vite + TS (components/, hooks/, lib/)
website                 static landing page (no build step)
docs/                   design + product/architecture plans
```
The `crates/compas-*` core (core/dsp/sources/script) is **product-agnostic** — Compás Studio will
reuse it. `crates/compas-audio` is the **DJ-specific** engine; the DAW will bring its own engine.
`Cargo.toml` `default-members` = the four engine crates, so `cargo check/test/clippy` skip the
Tauri app (which needs WebView2/WebKitGTK + a built frontend).

## Agent coordination (Claude + Codex)
This repo is built by multiple AI agents working in parallel (Claude as lead + Codex for delegated
tasks). To avoid collisions:
- **One file, one agent at a time.** Split parallel work by *file*, not by feature — name the exact
  files each agent may touch. The React deck/waveform/library/mixer/settings files overlap, and
  `apps/compas-dj/frontend/src/styles.css` is shared; keep shared-file edits in one agent's lane per
  round and have the other rebase.
- **Delegated tasks** get a brief in `docs/codex-tasks/CODEX-<x>.md` (scope, allowed/forbidden files,
  acceptance criteria). The delegate commits on a branch for review — never to `main` directly.
- **The lead reviews delegated work** against the brief before merge: read the whole changed surface
  + callers/tests (not just the diff), confirm `tsc`/`build`/`clippy` are green, then fast-forward.
- **No surprise GitHub writes.** Announce pushes/PRs/tags before doing them; commit or push only when
  the maintainer asks. Branch off `main` for non-trivial work.

## Commands
```bash
cargo test                                   # engine unit tests
cargo clippy --all-targets -- -D warnings    # lint engine crates
cargo fmt --all
cargo bench -p compas-dsp                    # DSP hot-path benchmarks
node scripts/check-versions.mjs              # Cargo/Tauri/frontend version consistency
cd apps/compas-dj/frontend && npm install && npm run typecheck && npm run build
# Run the full DJ app from the product dir (so the Tauri CLI finds its src-tauri sibling):
#   cd apps/compas-dj && frontend/node_modules/.bin/tauri dev       (macOS/Linux)
#   cd apps/compas-dj &&  frontend\node_modules\.bin\tauri.cmd dev  (Windows)
node scripts/make-test-audio.mjs             # synth 120/128 BPM test WAVs into samples/
pwsh scripts/install-hooks.ps1               # opt in to local pre-commit checks
```

## Non-negotiables
1. **Audio-thread real-time safety.** The cpal callback must never allocate, lock, block, log, or
   panic. RT-safe functions carry an `RT-SAFE` doc-comment. Cross-thread audio moves only through
   `rtrb` SPSC rings; control changes through the command ring. See `ARCHITECTURE.md` §8.
2. **No `unwrap()`/`expect()`/`panic!` in non-test code.** `Result` everywhere.
3. **Capability honesty (a product value, not a detail).** Local files = full DSP; streaming =
   control-only. Never render a DSP control for audio we don't decode. Locked state is driven by
   `SourceCapabilities` / a `dsp` prop, never hard-coded.
4. **Cross-platform from commit one.** Gate platform-specific code; document it.

## Rust discipline
- Prefer `#[expect(lint, reason = "...")]` over `#[allow(lint)]`; stale suppressions should fail.
- Every `unsafe` block must have a nearby `// SAFETY:` comment explaining the invariant.
- Use `cargo update -p crate --precise version` for targeted dependency updates; avoid bulk updates
  unless the task is explicitly dependency maintenance.
- Copy the closest existing test pattern before inventing a new one. For DSP/audio changes, include
  a deterministic unit test or benchmark when behavior or hot-path cost can regress.
- Keep `Cargo.toml`, `apps/compas-dj/src-tauri/tauri.conf.json`, and
  `apps/compas-dj/frontend/package.json` versions in sync.

## Refactoring discipline
- A refactor should **reduce** non-test LOC unless it removes a larger architectural cost; treat
  growing production LOC as a smell.
- **Delete the old path** — don't leave shims, aliases, dead CSS, or fallback stacks "just in case."
  Removal is the default unless the maintainer asks for an explicitly-named compat path.
- Prefer **closed shapes** (an `enum` / one result type) over parallel `Option` fields + derived
  booleans callers must keep in sync — make impossible states unrepresentable.
- Export the **smallest** surface a caller needs; keep helpers/types local until a second caller
  appears.

## Architecture cheatsheet
- Decks hold the **fully-decoded track in RAM** (`Arc<DeckBuffer>`); the audio thread reads with a
  cubic-Hermite **fractional play-head** advancing by `(source_rate/device_rate) × tempo`.
- Two output paths that **cannot** be summed in software: the cpal/DSP bus (local PCM) and the
  webview/OS audio (streaming SDKs).
- Streaming auth is **Authorization Code + PKCE** (no secret); Rust runs a loopback catcher.

Keep `ARCHITECTURE.md`, `ROADMAP.md`, and `CHANGELOG.md` updated alongside code.
