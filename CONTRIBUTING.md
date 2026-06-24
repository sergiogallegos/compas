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
cd apps/compas-dj/frontend && npm run typecheck && npm run build
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
**Compás** is a product family on a shared Rust core. `crates/compas-{core,dsp,sources,script}` is
the product-agnostic core; `crates/compas-audio` is the **Compás DJ** engine. The app lives in
`apps/compas-dj/{src-tauri,frontend}`; `website` is the landing page. A **Compás Studio** DAW is
planned (`docs/COMPAS-STUDIO-PLAN.md`). See `AGENTS.md` for a fuller orientation.

## Release setup (one-time, maintainer only)
Tagging `v*` triggers `.github/workflows/release.yml`, which builds Windows + macOS installers
and (when signing keys are configured) publishes a signed `latest.json` that the in-app
auto-updater consumes. To finish wiring a real release:

1. **Generate the updater signing keypair** (once per project; keep the private key offline):

   ```bash
   cd apps/compas-dj/frontend
   npx tauri signer generate -w ~/.tauri/compas.key
   ```

   This writes `~/.tauri/compas.key` (private) and `~/.tauri/compas.key.pub` (public).

2. **Paste the public key** into `apps/compas-dj/src-tauri/tauri.conf.json` at `plugins.updater.pubkey`
   (replacing the `REPLACE_BEFORE_RELEASE_…` placeholder).

3. **Add repo secrets** (Settings → Secrets and variables → Actions):
   - `TAURI_SIGNING_PRIVATE_KEY` — contents of `~/.tauri/compas.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the passphrase set during generation

4. **macOS signing + notarization** (optional; required for Gatekeeper-clean distribution):
   - `APPLE_CERTIFICATE` — base64 of the exported `Developer ID Application` `.p12`
   - `APPLE_CERTIFICATE_PASSWORD` — `.p12` export password
   - `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Your Name (TEAMID)`
   - `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`

5. **Windows code-signing** (optional): add `WINDOWS_CERTIFICATE` (base64 `.pfx`) +
   `WINDOWS_CERTIFICATE_PASSWORD`, or wire Azure Trusted Signing as a separate workflow
   step before `tauri-action`.

Without these the workflow still produces unsigned installers — useful for early previews.
The in-app updater will refuse unsigned `latest.json`, so users won't auto-update until step 3
is complete.
