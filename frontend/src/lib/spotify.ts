// Spotify Authorization Code + PKCE (public client — no secret). Rust runs the loopback
// listener (spotify_listen) and opens the browser; this module does PKCE, the token
// exchange/refresh, and Web API calls. Tokens live in localStorage for now; a later pass
// can move them to the OS keychain.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

const REDIRECT = "http://127.0.0.1:14565/callback";
const AUTH_URL = "https://accounts.spotify.com/authorize";
const TOKEN_URL = "https://accounts.spotify.com/api/token";
const SCOPES =
  "streaming user-read-email user-read-private user-modify-playback-state user-read-playback-state user-read-currently-playing";

const CLIENT_ID_KEY = "spotify_client_id";
const TOKENS_KEY = "spotify_tokens";

export function getClientId(): string {
  return localStorage.getItem(CLIENT_ID_KEY) ?? "";
}
export function setClientId(id: string): void {
  localStorage.setItem(CLIENT_ID_KEY, id.trim());
}

interface Tokens {
  access: string;
  refresh: string;
  expiresAt: number;
}
function loadTokens(): Tokens | null {
  const s = localStorage.getItem(TOKENS_KEY);
  return s ? (JSON.parse(s) as Tokens) : null;
}
function saveTokens(t: Tokens): void {
  localStorage.setItem(TOKENS_KEY, JSON.stringify(t));
}
export function isConnected(): boolean {
  return loadTokens() !== null;
}
export function disconnect(): void {
  localStorage.removeItem(TOKENS_KEY);
}

// ---- PKCE helpers ----
function b64url(buf: ArrayBuffer): string {
  let s = "";
  for (const b of new Uint8Array(buf)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function randomVerifier(): string {
  const arr = new Uint8Array(64);
  crypto.getRandomValues(arr);
  return b64url(arr.buffer).slice(0, 96);
}
async function challenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return b64url(digest);
}

/** Full connect flow: listen → open browser → catch code → exchange for tokens. */
export async function connect(): Promise<void> {
  const clientId = getClientId();
  if (!clientId) throw new Error("Set your Spotify Client ID first.");
  const verifier = randomVerifier();
  const chal = await challenge(verifier);

  const code = await new Promise<string>((resolve, reject) => {
    let un: UnlistenFn | null = null;
    const timer = setTimeout(() => {
      un?.();
      reject(new Error("login timed out"));
    }, 190_000);

    listen<{ code?: string; error?: string }>("spotify:code", (e) => {
      clearTimeout(timer);
      un?.();
      if (e.payload.code) resolve(e.payload.code);
      else reject(new Error(e.payload.error ?? "login failed"));
    }).then((u) => {
      un = u;
    });

    invoke("spotify_listen")
      .then(() => {
        const params = new URLSearchParams({
          client_id: clientId,
          response_type: "code",
          redirect_uri: REDIRECT,
          scope: SCOPES,
          code_challenge_method: "S256",
          code_challenge: chal,
        });
        return openUrl(`${AUTH_URL}?${params.toString()}`);
      })
      .catch((e) => {
        clearTimeout(timer);
        un?.();
        reject(e instanceof Error ? e : new Error(String(e)));
      });
  });

  await exchangeCode(code, verifier, clientId);
}

async function exchangeCode(code: string, verifier: string, clientId: string): Promise<void> {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: REDIRECT,
    client_id: clientId,
    code_verifier: verifier,
  });
  const res = await fetch(TOKEN_URL, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body,
  });
  if (!res.ok) throw new Error(`token exchange failed (${res.status})`);
  const d = (await res.json()) as { access_token: string; refresh_token: string; expires_in: number };
  saveTokens({ access: d.access_token, refresh: d.refresh_token, expiresAt: Date.now() + d.expires_in * 1000 });
}

/** Returns a valid access token, refreshing if needed; null if not connected. */
export async function accessToken(): Promise<string | null> {
  const t = loadTokens();
  if (!t) return null;
  if (Date.now() < t.expiresAt - 60_000) return t.access;

  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: t.refresh,
    client_id: getClientId(),
  });
  const res = await fetch(TOKEN_URL, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body,
  });
  if (!res.ok) {
    disconnect();
    return null;
  }
  const d = (await res.json()) as { access_token: string; refresh_token?: string; expires_in: number };
  const next: Tokens = {
    access: d.access_token,
    refresh: d.refresh_token ?? t.refresh,
    expiresAt: Date.now() + d.expires_in * 1000,
  };
  saveTokens(next);
  return next.access;
}

export interface SpotifyTrack {
  id: string;
  uri: string;
  title: string;
  artist: string;
  album: string;
  durationMs: number;
  art?: string;
}

interface RawTrack {
  id: string;
  uri: string;
  name: string;
  duration_ms: number;
  artists?: { name: string }[];
  album?: { name?: string; images?: { url: string }[] };
}

export async function search(q: string): Promise<SpotifyTrack[]> {
  const token = await accessToken();
  if (!token || q.trim() === "") return [];
  const res = await fetch(`https://api.spotify.com/v1/search?type=track&limit=20&q=${encodeURIComponent(q)}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!res.ok) throw new Error(`search failed (${res.status})`);
  const d = (await res.json()) as { tracks?: { items: RawTrack[] } };
  return (d.tracks?.items ?? []).map((t) => {
    const imgs = t.album?.images ?? [];
    return {
      id: t.id,
      uri: t.uri,
      title: t.name,
      artist: t.artists?.[0]?.name ?? "",
      album: t.album?.name ?? "",
      durationMs: t.duration_ms,
      art: imgs.length > 0 ? imgs[imgs.length - 1].url : undefined,
    };
  });
}
