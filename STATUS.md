# compas — status & resume point

> Checkpoint for picking work back up. Last updated: 2026-06-17. See `ROADMAP.md` for the
> full plan, `CHANGELOG.md` for history, `AGENTS.md` for conventions.

## ▶ Resume here (next up, in order)
1. **Jog-wheel scratch + rotating platter** ← agreed next feature. Make the platter a draggable,
   spinning disc; dragging drives the play-head (scratch, forward + reverse) and a nudge. Our
   in-RAM fractional play-head is the right substrate; for smooth scratch, drive the audio-thread
   read-rate from drag velocity (not just coarse seeks).
2. **FX rack** — echo/delay first (pre-allocated delay line, RT-safe), then reverb; enable the
   deck FX row (ECHO/REVERB). Filter already exists.
3. **Auto-update** (`tauri-plugin-updater`) + **release code-signing** — generate the signing
   keypair (`npm run tauri signer generate`), put pubkey in `tauri.conf.json`, privkey + password
   as CI secrets, uncomment the signing env in `release.yml`, add a "Check for updates" UI.
4. **`vergen`** build/git version in the status bar (replaces hardcoded `compas 0.1.0`).
5. **FFmpeg fallback decode** for formats symphonia can't handle (e.g. MPEG *video* containers) —
   only if a real format gap shows up.
6. **P1 remainders:** key-lock toggle (signalsmith-stretch), manual beatgrid-anchor edit, underrun
   counters in the UI, a decode-a-fixture integration test, 4-deck layout.

## ✅ Done
**P0 scaffold** · Tauri 2 workspace, 4 engine crates, CI-green.

**P1 — local dual-deck engine (functionally complete):**
- Decode (symphonia, in-RAM `DeckBuffer`), 2 decks, transport (play/pause/cue/seek), crossfader,
  per-deck gain, 3-band EQ, HPF/LPF filter, varispeed + nudge, one-shot tempo **SYNC**.
- **BPM + beatgrid + musical key** (Camelot) analysis on load.
- **Beat loops** (IN/OUT manual + 4/8/16 grid-snapped; waveform loop band).
- **Hot cues** (set/jump/clear).
- Scrolling **zoom waveforms** (fixed NOW, beat grid, 4–32 s), VU metering.
- **Local library** (add files → persisted; search; load A/B / double-click; remove) + load
  progress feedback.
- Full performance UI (frameless window + traffic-light controls), brand mark + icons.

**P2a — Spotify (built, then parked):** Authorization Code + PKCE auth + catalog search exist in
code (`src-tauri/src/spotify.rs`, `frontend/src/lib/spotify.ts`, `useSpotify.ts`) but the UI
sources are **disabled** per request. See "Parked" below.

**Infra / OSS:** MIT `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AGENTS.md`, `rust-toolchain.toml`,
`rustfmt.toml`, CI (`.github/workflows/ci.yml`: fmt/clippy/test/frontend/audit), `release.yml`
(Win/macOS installers on `v*` tag; signing commented), `audit.toml`, criterion DSP benches,
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
cargo test ; cargo clippy --all-targets -- -D warnings    # engine
cd frontend && npm install && npm run typecheck && npm run build
cargo tauri dev            # full app (or frontend\node_modules\.bin\tauri.cmd dev)
node scripts/make-test-audio.mjs    # 120/128 BPM test WAVs -> samples/
```
