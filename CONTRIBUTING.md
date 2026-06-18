# Contributing to compas

Thanks for your interest! compas is a cross-platform real-time DJ app (Rust core + TypeScript UI
in Tauri 2), open-source under the MIT license — contributions are welcome.

## Dev setup
See `README.md` for prerequisites (Rust ≥ 1.82, Node ≥ 18, WebView2 / Xcode CLT, Tauri CLI).

## Before you push
The engine crates build/test without WebView2 or a frontend build:

```bash
cargo fmt --all                              # format
cargo clippy --all-targets -- -D warnings    # lint (engine crates / default-members)
cargo test                                   # DSP / source / analysis tests
cd frontend && npm run typecheck && npm run build
```

CI runs the same on Windows/macOS/Linux; keep it green.

## Conventions
- **Real-time safety is non-negotiable.** Nothing in the audio callback may allocate, lock, block,
  log, or panic. Functions safe to call there carry an `RT-SAFE` doc-comment — respect the contract.
  See `ARCHITECTURE.md` §8.
- **Error handling:** `Result`-based; no `unwrap()`/`expect()` in non-test code.
- **Capability honesty:** never surface a DSP control for a source whose audio we don't decode
  (streaming). Locked states are data-driven (`SourceCapabilities` / `dsp` props), not hard-coded.
- **Cross-platform from commit one;** gate platform-specific code and document it.
- **Commits:** small and [Conventional](https://www.conventionalcommits.org/) (`feat:`, `fix:`,
  `docs:`, `chore:`…). Keep `ARCHITECTURE.md`, `ROADMAP.md`, and `CHANGELOG.md` current with changes.
- Tests for DSP/analysis units; document real-time assumptions on audio-thread code.

## Project map
`crates/compas-{core,dsp,audio,sources}` (engine), `src-tauri` (Tauri app + IPC),
`frontend` (React UI), `website` (landing page). See `AGENTS.md` for a fuller orientation.
