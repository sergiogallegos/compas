# djvibebar review — reuse assessment for compas

Reviewed: `C:\Users\Sergio Gallegos\projects\djvibebar` (commit `d759b81`).
Purpose: decide what streaming-integration work we port into compas, and what we leave behind.

## TL;DR

djvibebar is **not** a Tauri desktop app. It is a **client/server party-jukebox SaaS**:
an `axum` HTTP backend (Rust) + a React 19 / Vite SPA + a Capacitor iOS wrapper, backed by
Postgres (`sqlx`), Clerk auth, and Stripe. Music is handled under a "BYO-License" model:
the venue plays from *their own* Spotify/Apple/SoundCloud account, and the app only does
**metadata search + a request queue + playback control**. It never touches decoded audio.

That means the *reusable* surface for compas is narrow but real: the **OAuth/token logic**,
the **search/metadata clients**, the **Apple developer-token generation**, the **normalized
track model**, and the **frontend SDK lifecycle code**. Everything else (queue economics,
coins, Stripe, moderation, multi-tenant DB, Clerk, iOS) is irrelevant to a local DJ app.

There is also one **blocking architectural mismatch** and two **hard external constraints**
you need to know before Phase 2 (details in §5–6):

1. djvibebar uses **confidential-client OAuth** (a `client_secret` baked into the backend).
   A distributable desktop binary cannot hold a secret — we must move Spotify & SoundCloud
   to **Authorization Code + PKCE** (public client). The request/refresh logic ports; the
   transport does not.
2. The Spotify/Apple SDKs are **browser playback controllers with no PCM access** — confirmed
   directly in the code. No DSP on their audio is possible, ever.
3. djvibebar fetches **no tempo/beat/key data at all**, and Spotify's `audio-features`/
   `audio-analysis` endpoints are **deprecated for apps registered after 2024-11-27**. The
   Phase-3 "use Spotify beat data to time transitions" plan probably has no data source.

---

## 1. Structure & how the halves communicate

```
djvibebar/
  backend/   axum 0.8 HTTP server (Rust). sqlx+Postgres, Clerk JWT, Stripe, tower_governor.
             "HTTP polling architecture" (no websockets).
  frontend/  React 19 + Vite 6 + TS + Tailwind. Capacitor for iOS.
  ios/       Capacitor shell.
```

- **Transport:** plain REST/JSON over HTTP. Frontend hits `${VITE_API_URL}/api/...`. State is
  synced by **polling**, not push.
- **Backend module map (streaming-relevant only):** `spotify.rs` / `spotify_auth.rs` /
  `spotify_routes.rs`, `soundcloud.rs` / `soundcloud_auth.rs` / `soundcloud_routes.rs`,
  `apple_music_auth.rs`, `music_provider.rs` (a small capability enum), `state.rs` (shared
  `AppState`). The rest is jukebox/SaaS domain — see §4.
- **OAuth return path:** service → backend callback → backend stores a one-time **"handoff key"**
  in an in-memory `DashMap` → redirects to the SPA with `?<svc>_handoff=<key>` → SPA calls
  `/api/<svc>/handoff?key=...` to exchange it for the token → token lands in browser
  `localStorage`. iOS variant redirects to a `djvibebar://` custom scheme.

## 2. Auth flows, per service

| Service | Flow | Secret? | Token store | Notes |
|---|---|---|---|---|
| **Spotify** | Authorization Code (user playback) **+** client_credentials (app metadata search) | **Yes** (`client_secret`, backend) | `localStorage` (user); in-memory `RwLock` cache (app token) | Scopes: `streaming user-read-email user-read-private user-modify-playback-state user-read-playback-state`. `show_dialog=true`. Refresh endpoint. **No PKCE. No CSRF state** (only a `cap` marker). |
| **SoundCloud** | Authorization Code **+** client_credentials **+** legacy client_id fallback | **Yes** | `localStorage`; host token can persist in Postgres `bars.settings` JSONB | **Has CSRF `state`** validation (`DashSet`). scope `non-expiring`. Refresh endpoint. |
| **Apple Music** | **No OAuth.** Backend mints **ES256 developer JWT** from a `.p8` key (`jsonwebtoken`); frontend uses **MusicKit JS** for user auth + playback. | n/a (private key, backend) | MusicKit-managed | 1-hour developer token. `load_private_key` handles inline-PEM / escaped-`\n` / raw-base64 / file-path. Search proxied server-side. |

## 3. API clients, types, rate-limit handling

- **`SpotifyService`** (`spotify.rs`): caches the client_credentials token (refresh 5 min before
  expiry), calls `/v1/search`. Quirk worth keeping: it **omits `limit`** because passing it
  triggered spurious `400 Invalid limit`. Maps results into the normalized track model.
- **`SoundCloudService`** (`soundcloud.rs`): 3-strategy search (user OAuth → client_credentials →
  client_id), `api.soundcloud.com/tracks`, artwork upscaling (`-large` → `-t500x500`), and a
  paginated-vs-array response fallback parser.
- **Apple** (`apple_music_auth.rs`): developer-token endpoint + catalog search proxy, artwork
  `{w}x{h}` templating, exposes 30-second `preview_url` (DRM-free AAC).
- **Normalized model** (`queue.rs::TrackMetadata`, mirrored in `frontend/src/types/music.ts`):
  `{ id, spotify_id, title, artist, album_name?, album_art_url?, duration_ms?, source_provider? }`.
  ⚠️ `spotify_id` is a **misnomer** — it also stores SoundCloud/Apple IDs.
- **Rate limiting:** minimal. App-token caching avoids re-auth. The **frontend** `playTrack`
  handles `429` (single `Retry-After` retry) and `403` (checks `/v1/me.product` to detect Free
  accounts). The backend rate-limits *its own* endpoints with `tower_governor`, but has **no
  backoff/jitter on provider `429`s**. No retry budget, no circuit breaker.
- **No analysis data:** a full grep found **zero** use of `audio-features`, `audio-analysis`,
  tempo, BPM, beats, or key anywhere in the repo.

## 4. Reusable vs. leave-behind

**Reuse (port into compas):**
- Apple Music **ES256 developer-token generation** — almost verbatim in Rust (`jsonwebtoken`).
- Spotify & SoundCloud **token-exchange / refresh request shapes** (endpoints, form params,
  response structs) — reuse the logic; swap the client type to PKCE (§5).
- **Search clients + response-parsing structs** for all three services.
- The **normalized track model** — already ported and corrected into `compas-core::TrackMetadata`
  (`provider_id` + explicit `provider`, plus `bpm`/`musical_key` slots).
- **Frontend SDK lifecycle patterns**: ready/not-ready/error handling, Premium detection,
  global-singleton player, `429`/`403` handling — directly reusable for the Phase-2 streaming deck.
- The capability-enum *idea* in `music_provider.rs` — generalized into
  `compas-core::SourceCapabilities` (`full_dsp` vs `playback_only`).

**Leave behind:**
- The entire jukebox/SaaS domain: `queue*`, `coin_routes`, `queue_economics`, `stripe*`,
  `bar_config`, `bar_safety`, `vibe_guard`, `moderation`, `battle_routes`, `map_routes`,
  `admin_routes`, `favorite_routes`, `refund`, `content_controls`, `user_resolution`, shoutbox.
- **Postgres + `sqlx` + migrations** (multi-tenant) — compas uses local storage (SQLite/files).
- **Clerk** auth / JWKS — no user accounts in a personal desktop app.
- **Capacitor / iOS**.
- The **hosted-backend assumptions**: server-held `client_secret`, hosted HTTPS redirect URIs,
  and the server-side handoff-key indirection — all unnecessary when token exchange happens in
  the local Rust process.
- The `spotify_id` misnomer and the "BYO-License" framing.

## 5. Blocking port constraint: confidential client → PKCE

A desktop binary can be unpacked, so it cannot embed a `client_secret`. Both Spotify and
SoundCloud must move to **Authorization Code with PKCE** (public client, no secret):

- Redirect to a **loopback** `http://127.0.0.1:<ephemeral>/callback` (Spotify supports loopback
  for desktop) or a registered custom scheme; run a tiny local listener for the code.
- Do the **token exchange + refresh in the local Rust process** (reqwest — same calls djvibebar
  already makes, minus `basic_auth(secret)`, plus `code_verifier`).
- Store tokens in the **OS keychain** (`keyring` crate, or `tauri-plugin-stronghold`), not
  `localStorage`.

Apple Music's developer-token approach is unaffected, **but** it ships your `.p8` private key —
on desktop that key would be extractable, so it should live behind your own minimal token
endpoint or be treated as a personal-use-only secret. (Apple is playback-only regardless; see §6.)

## 6. External constraints you must design around (honest version)

- **No PCM from Spotify/Apple.** Confirmed in `useSpotifyPlayback.ts` (loads `sdk.scdn.co`,
  drives play/pause/seek/volume via Web API) and `apple_music_auth.rs` (MusicKit JS). These are
  **playback controllers**. No EQ, filter, time-stretch, beat-sync, or scratch on their audio is
  possible — not a limitation we can engineer around.
- **Spotify analysis endpoints are gone for new apps.** `audio-features`/`audio-analysis` were
  deprecated 2024-11-27 for newly-created apps. A compas Spotify app registered today will most
  likely **not** get tempo/beat/key data. Phase 3's "time transitions using Spotify beat data"
  needs a different plan: we can only schedule streaming transitions on **track position +
  user-supplied/our-own BPM**, never sample-accurate beat-sync.
- **Premium required** for the Web Playback SDK; one active device at a time.
- **SoundCloud is the one partial exception.** Its API can return real stream URLs (progressive
  MP3 / HLS) for tracks the uploader marked streamable — audio that *could* be decoded to PCM and
  run through full DSP. But: ToS restricts altering/redistribution, the catalog is uneven, and
  URLs are short-lived/often HLS. Treat as a documented, opt-in, **personal-use** path — not an
  architectural guarantee.
- **Non-commercial posture.** Staying control-only on streaming (never capturing their PCM) keeps
  us inside the SDKs' playback envelope. True mixing/DSP is reserved for **local DRM-free files**.
  This split is the core of compas's architecture, not a temporary limitation.
