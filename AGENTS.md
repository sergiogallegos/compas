# AGENTS.md — orientation for AI agents & contributors

Read this before making changes. It's the fast path to being productive in compas without breaking
the things that matter.

## What compas is
A cross-platform, real-time professional DJ app. **Rust audio core + TypeScript UI in Tauri 2.**
Windows is the primary target; macOS is first-class; Linux best-effort. Personal, non-commercial use.

## Repository map
```
crates/compas-core      shared types (TrackMetadata, SourceCapabilities, DeckBuffer, errors)
crates/compas-dsp       real-time-safe DSP (rt::) + offline analysis (analysis:: BPM/beatgrid/key)
crates/compas-audio     cpal engine, lock-free rings, Mixer, telemetry, waveform peaks
crates/compas-sources   AudioSource abstraction: LocalFileSource (PcmSource) + StreamingSource
src-tauri               Tauri app: IPC commands, audio thread bridge, Spotify auth (spotify.rs)
frontend                React + Vite + TS UI (components/, hooks/, lib/)
website                 static landing page (no build step)
docs/                   djvibebar review, design assets
```
`Cargo.toml` `default-members` = the four engine crates, so `cargo check/test/clippy` skip the
Tauri app (which needs WebView2/WebKitGTK + a built frontend).

## Commands
```bash
cargo test                                   # engine unit tests
cargo clippy --all-targets -- -D warnings    # lint engine crates
cargo fmt --all
cd frontend && npm install && npm run typecheck && npm run build
cargo tauri dev                              # run the full app (or frontend\node_modules\.bin\tauri.cmd dev)
node scripts/make-test-audio.mjs             # synth 120/128 BPM test WAVs into samples/
```

## Non-negotiables
1. **Audio-thread real-time safety.** The cpal callback must never allocate, lock, block, log, or
   panic. RT-safe functions carry an `RT-SAFE` doc-comment. Cross-thread audio moves only through
   `rtrb` SPSC rings; control changes through the command ring. See `ARCHITECTURE.md` §8.
2. **No `unwrap()`/`expect()`/`panic!` in non-test code.** `Result` everywhere.
3. **Capability honesty (a product value, not a detail).** Local files = full DSP; streaming =
   control-only. Never render a DSP control for audio we don't decode. Locked state is driven by
   `SourceCapabilities` / a `dsp` prop, never hard-coded. See `docs/djvibebar-review.md` §6.
4. **Cross-platform from commit one.** Gate platform-specific code; document it.

## Architecture cheatsheet
- Decks hold the **fully-decoded track in RAM** (`Arc<DeckBuffer>`); the audio thread reads with a
  cubic-Hermite **fractional play-head** advancing by `(source_rate/device_rate) × tempo`.
- Two output paths that **cannot** be summed in software: the cpal/DSP bus (local PCM) and the
  webview/OS audio (streaming SDKs).
- Streaming auth is **Authorization Code + PKCE** (no secret); Rust runs a loopback catcher.

Keep `ARCHITECTURE.md`, `ROADMAP.md`, and `CHANGELOG.md` updated alongside code.
