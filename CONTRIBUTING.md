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

## AI-assisted contributions
Agent-written and AI-assisted PRs are first-class here (much of compas is built this way). To keep
them reviewable:
- **Mark it** AI-assisted in the PR title or description.
- **Include an Evidence section** — the most useful validation, not a narrative: which of
  `cargo test` / `clippy -D warnings` / `tsc` / `vite build` you ran and that they're green, plus any
  in-app check. Reviewers trust inspected code, tests, and CI over prose.
- **Confirm you understand the change** — be able to explain what it does and why.
- **Self-review first.** Run `/code-review` (or your agent's review) against `origin/main` and address
  findings before requesting human review; resolve/reply to review-bot threads rather than leaving
  cleanup for the maintainer.
- **No surprise GitHub writes** — don't push/tag/open PRs the maintainer didn't ask for. Delegated
  agents commit on a branch for review (see `AGENTS.md` § Agent coordination), never to `main`.

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
Tagging `v*` triggers `.github/workflows/release.yml`, which builds Windows, macOS, and Linux
installers and publishes a signed `latest.json` (updater signature) to a **draft** GitHub Release.

> **Current state (v0.1.0):** the **updater key is configured** (passwordless minisign key in the
> `TAURI_SIGNING_PRIVATE_KEY` secret; pubkey baked into `tauri.conf.json`), so the build is green and
> signs `latest.json`. But the installers are **not OS code-signed** — there's no Apple Developer ID
> or Windows code-signing cert yet, so users get first-launch warnings (Windows "Run anyway"; macOS
> *"damaged"* → `xattr -cr "/Applications/Compás DJ.app"`). The `APPLE_*` env block in `release.yml`
> is intentionally disabled (passing empty cert secrets breaks the macOS bundle); re-enable it once a
> Developer ID exists. Steps 1–3 below (the updater key) are **done**; steps 4–5 (OS signing) are
> pending paid certs.

To (re)wire signing:

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
