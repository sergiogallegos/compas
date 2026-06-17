// Typed wrappers over Tauri commands + events for the compas engine.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

/** True when running inside the Tauri webview (vs. a plain browser dev tab). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

// ---- Event payloads (mirror src-tauri/src/lib.rs) ---------------------------------

export interface DeckLoaded {
  deck: number;
  title: string;
  artist: string;
  duration_ms: number;
  source_rate: number;
  frames: number;
  bpm: number;
  bpm_confidence: number;
  first_beat_sec: number;
  beat_interval_sec: number;
  key_camelot: string;
  key_name: string;
  peaks: number[];
}

export interface DeckPosition {
  deck: number;
  frame: number;
  playing: boolean;
  level: number;
}

export interface MasterMeter {
  l: number;
  r: number;
}

export interface DeckError {
  deck: number;
  message: string;
}

export interface EngineStatus {
  sample_rate: number;
  decks: { deck: number; loaded: boolean; playing: boolean; frame: number }[];
}

export type FilterMode = "off" | "lowpass" | "highpass";

// ---- Commands ---------------------------------------------------------------------

export async function engineStatus(): Promise<EngineStatus> {
  return invoke<EngineStatus>("engine_status");
}

/** Open a native file picker; returns the chosen path or null. */
export async function pickAudioFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Audio", extensions: ["mp3", "flac", "wav", "ogg", "m4a", "aac", "aiff"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export const loadTrack = (deck: number, path: string) => invoke("load_track", { deck, path });
export const deckPlay = (deck: number) => invoke("deck_play", { deck });
export const deckPause = (deck: number) => invoke("deck_pause", { deck });
export const deckSeek = (deck: number, frame: number) => invoke("deck_seek", { deck, frame });
export const deckUnload = (deck: number) => invoke("deck_unload", { deck });
export const setDeckTempo = (deck: number, ratio: number) =>
  invoke("set_deck_tempo", { deck, ratio });
export const setDeckGain = (deck: number, gain: number) => invoke("set_deck_gain", { deck, gain });
export const setDeckEq = (deck: number, low: number, mid: number, high: number) =>
  invoke("set_deck_eq", { deck, low, mid, high });
export const setDeckFilter = (deck: number, mode: FilterMode, cutoff: number, resonance: number) =>
  invoke("set_deck_filter", { deck, mode, cutoff, resonance });
export const setCrossfader = (value: number) => invoke("set_crossfader", { value });
export const setMasterGain = (value: number) => invoke("set_master_gain", { value });

// ---- Event subscriptions ----------------------------------------------------------

export const onDeckLoaded = (cb: (e: DeckLoaded) => void): Promise<UnlistenFn> =>
  listen<DeckLoaded>("deck:loaded", (e) => cb(e.payload));
export const onDeckPosition = (cb: (e: DeckPosition) => void): Promise<UnlistenFn> =>
  listen<DeckPosition>("deck:position", (e) => cb(e.payload));
export const onDeckError = (cb: (e: DeckError) => void): Promise<UnlistenFn> =>
  listen<DeckError>("deck:error", (e) => cb(e.payload));
export const onMasterMeter = (cb: (e: MasterMeter) => void): Promise<UnlistenFn> =>
  listen<MasterMeter>("master:level", (e) => cb(e.payload));
