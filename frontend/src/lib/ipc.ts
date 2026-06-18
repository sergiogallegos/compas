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
  path: string;
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

const AUDIO_FILTERS = [
  {
    name: "Audio",
    extensions: [
      "mp3", "mpeg", "mpga", "mp2", "flac", "wav", "wave", "ogg", "oga", "opus",
      "m4a", "mp4", "aac", "adts", "aif", "aiff", "aifc",
    ],
  },
  { name: "All Files", extensions: ["*"] },
];

/** Open a native file picker; returns the chosen path or null. */
export async function pickAudioFile(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: false, filters: AUDIO_FILTERS });
  return typeof selected === "string" ? selected : null;
}

/** Open a multi-select picker for adding tracks to the library. */
export async function pickAudioFiles(): Promise<string[]> {
  const selected = await open({ multiple: true, directory: false, filters: AUDIO_FILTERS });
  if (Array.isArray(selected)) return selected;
  return typeof selected === "string" ? [selected] : [];
}

export interface ProbedTrack {
  path: string;
  title: string;
  artist: string;
  duration_ms: number;
}

/** Cheap header probe for adding a file to the library (no full decode). */
export const probeTrack = (path: string) => invoke<ProbedTrack>("probe_track", { path });

export const loadTrack = (deck: number, path: string) => invoke("load_track", { deck, path });
export const deckPlay = (deck: number) => invoke("deck_play", { deck });
export const deckPause = (deck: number) => invoke("deck_pause", { deck });
export const deckSeek = (deck: number, frame: number) => invoke("deck_seek", { deck, frame });
export const deckUnload = (deck: number) => invoke("deck_unload", { deck });
export const setLoop = (deck: number, inFrame: number, outFrame: number, active: boolean) =>
  invoke("set_loop", { deck, inFrame, outFrame, active });
export const setLoopActive = (deck: number, active: boolean) =>
  invoke("set_loop_active", { deck, active });
/** Jog-wheel scratch: speed 1.0 = natural play rate, negative = reverse, 0 = held. */
export const deckScratch = (deck: number, active: boolean, speed: number) =>
  invoke("deck_scratch", { deck, active, speed });
export const setDeckTempo = (deck: number, ratio: number) =>
  invoke("set_deck_tempo", { deck, ratio });
export const setDeckGain = (deck: number, gain: number) => invoke("set_deck_gain", { deck, gain });
export const setDeckEq = (deck: number, low: number, mid: number, high: number) =>
  invoke("set_deck_eq", { deck, low, mid, high });
export const setDeckFilter = (deck: number, mode: FilterMode, cutoff: number, resonance: number) =>
  invoke("set_deck_filter", { deck, mode, cutoff, resonance });
/** Echo/delay insert. `timeSec` is the delay time (UI beat-syncs it); feedback 0..0.95, mix 0..1. */
export const setDeckEcho = (
  deck: number,
  active: boolean,
  timeSec: number,
  feedback: number,
  mix: number,
) => invoke("set_deck_echo", { deck, active, timeSec, feedback, mix });
/** Reverb insert. `roomSize` 0..1 sets the tail length; `mix` 0..1 the wet/dry balance. */
export const setDeckReverb = (deck: number, active: boolean, roomSize: number, mix: number) =>
  invoke("set_deck_reverb", { deck, active, roomSize, mix });
export const setCrossfader = (value: number) => invoke("set_crossfader", { value });
export const setMasterGain = (value: number) => invoke("set_master_gain", { value });

// ---- Event subscriptions ----------------------------------------------------------

export interface DeckLoading {
  deck: number;
  path: string;
}
export const onDeckLoading = (cb: (e: DeckLoading) => void): Promise<UnlistenFn> =>
  listen<DeckLoading>("deck:loading", (e) => cb(e.payload));
export const onDeckLoaded = (cb: (e: DeckLoaded) => void): Promise<UnlistenFn> =>
  listen<DeckLoaded>("deck:loaded", (e) => cb(e.payload));
export const onDeckPosition = (cb: (e: DeckPosition) => void): Promise<UnlistenFn> =>
  listen<DeckPosition>("deck:position", (e) => cb(e.payload));
export const onDeckError = (cb: (e: DeckError) => void): Promise<UnlistenFn> =>
  listen<DeckError>("deck:error", (e) => cb(e.payload));
export const onMasterMeter = (cb: (e: MasterMeter) => void): Promise<UnlistenFn> =>
  listen<MasterMeter>("master:level", (e) => cb(e.payload));
