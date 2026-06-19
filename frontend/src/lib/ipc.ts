// Typed wrappers over Tauri commands + events for the compas engine.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

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

export interface EngineLoad {
  /** Audio-callback load, 0..~1 (≥1 = overrun). */
  load: number;
  /** Cumulative real-time-budget overruns since start. */
  xruns: number;
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

// ---- Library + per-track state (SQLite) -------------------------------------------

/** A library track row, including cached analysis + persisted gain/grid/play stats. */
export interface DbTrack {
  path: string;
  title: string;
  artist: string;
  duration_ms: number;
  bpm: number | null;
  key_camelot: string | null;
  key_name: string | null;
  grid_offset_sec: number;
  gain: number;
  play_count: number;
  last_played_at: number | null;
}
export interface DbCue {
  slot: number;
  frame: number;
}
export interface DbLoop {
  slot: number;
  in_frame: number;
  out_frame: number;
  beats: number | null;
}
/** Saved per-track performance state, restored when a track is reloaded onto a deck. */
export interface DbTrackState {
  grid_offset_sec: number;
  gain: number;
  cues: DbCue[];
  loops: DbLoop[];
}
export interface DbHistory {
  track_path: string;
  title: string;
  artist: string;
  played_at: number;
}

export const dbListTracks = () => invoke<DbTrack[]>("db_list_tracks");
export const dbAddTrack = (path: string) => invoke<DbTrack>("db_add_track", { path });
export const dbRemoveTrack = (path: string) => invoke("db_remove_track", { path });
export const dbTrackState = (path: string) => invoke<DbTrackState>("db_track_state", { path });
export const dbSetCue = (path: string, slot: number, frame: number) =>
  invoke("db_set_cue", { path, slot, frame });
export const dbClearCue = (path: string, slot: number) => invoke("db_clear_cue", { path, slot });
export const dbSetLoop = (
  path: string,
  slot: number,
  inFrame: number,
  outFrame: number,
  beats: number | null,
) => invoke("db_set_loop", { path, slot, inFrame, outFrame, beats });
export const dbClearLoop = (path: string, slot: number) => invoke("db_clear_loop", { path, slot });
export const dbSetGridOffset = (path: string, sec: number) =>
  invoke("db_set_grid_offset", { path, sec });
export const dbSetGain = (path: string, gain: number) => invoke("db_set_gain", { path, gain });
export const dbUpsertAnalysis = (t: DeckLoaded) =>
  invoke("db_upsert_analysis", {
    path: t.path,
    bpm: t.bpm,
    bpmConfidence: t.bpm_confidence,
    firstBeatSec: t.first_beat_sec,
    beatIntervalSec: t.beat_interval_sec,
    keyCamelot: t.key_camelot,
    keyName: t.key_name,
  });
export const dbRecordPlay = (path: string) => invoke("db_record_play", { path });
export const dbHistory = (limit: number) => invoke<DbHistory[]>("db_history", { limit });

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
/** Key-lock (master tempo): tempo changes preserve pitch. */
export const setDeckKeylock = (deck: number, active: boolean) =>
  invoke("set_deck_keylock", { deck, active });
/** Update a deck's beatgrid (in source frames) for the sync engine after a manual nudge. */
export const setBeatgrid = (deck: number, offsetFrames: number, intervalFrames: number) =>
  invoke("set_beatgrid", { deck, offsetFrames, intervalFrames });
/** Continuous beat-sync: `master` is the deck index to follow, or null to disengage. */
export const setDeckSync = (deck: number, master: number | null) =>
  invoke("set_deck_sync", { deck, master });
/** Route a deck to a crossfader side for 4-deck mixing: 0 = A, 1 = thru, 2 = B. */
export const setDeckXfaderAssign = (deck: number, assign: number) =>
  invoke("set_deck_xfader_assign", { deck, assign });
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

// ---- Synth instrument + MIDI ------------------------------------------------------

export const noteOn = (note: number, velocity: number) => invoke("note_on", { note, velocity });
export const noteOff = (note: number) => invoke("note_off", { note });
export const allNotesOff = () => invoke("all_notes_off");
export const setSynthWaveform = (index: number) => invoke("set_synth_waveform", { index });
export const setSynthGain = (gain: number) => invoke("set_synth_gain", { gain });
/** List connected MIDI input ports (index = position in the returned list). */
export const midiListPorts = () => invoke<string[]>("midi_list_ports");
/** Open a MIDI port; returns its name. Notes emit `midi:note` (+ drive the synth when
 *  enabled), CCs emit `midi:cc`. */
export const midiConnect = (index: number) => invoke<string>("midi_connect", { index });
export const midiDisconnect = () => invoke("midi_disconnect");
/** Whether incoming MIDI notes drive the synth. Off lets a controller map to deck controls
 *  (via `midi:note`) without honking the synth. */
export const setMidiSynth = (enabled: boolean) => invoke("set_midi_synth", { enabled });

export interface MidiCc {
  controller: number;
  value: number;
}
export const onMidiCc = (cb: (e: MidiCc) => void): Promise<UnlistenFn> =>
  listen<MidiCc>("midi:cc", (e) => cb(e.payload));

export interface MidiNote {
  note: number;
  velocity: number;
  on: boolean;
}
export const onMidiNote = (cb: (e: MidiNote) => void): Promise<UnlistenFn> =>
  listen<MidiNote>("midi:note", (e) => cb(e.payload));

// ---- Master recording -------------------------------------------------------------

/** Record the master mix to `path` (32-bit-float stereo WAV). */
export const startRecording = (path: string) => invoke("start_recording", { path });
export const stopRecording = () => invoke("stop_recording");

/** Native save dialog for the recording target; returns the chosen path or null. */
export async function pickRecordingPath(): Promise<string | null> {
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  const selected = await save({
    defaultPath: `compas-mix-${stamp}.wav`,
    filters: [{ name: "WAV audio", extensions: ["wav"] }],
  });
  return typeof selected === "string" ? selected : null;
}

// ---- Headphone / cue monitoring ---------------------------------------------------

/** List output devices the headphone cue can target (first is usually the system default). */
export const listOutputDevices = () => invoke<string[]>("list_output_devices");
/** Start cue monitoring on `device` (omit for the default output); returns the device name. */
export const startCueOutput = (device?: string) =>
  invoke<string>("start_cue_output", { device: device ?? null });
export const stopCueOutput = () => invoke("stop_cue_output");
/** Toggle pre-fader-listen (PFL) for a deck on the headphone cue bus. */
export const setDeckCue = (deck: number, active: boolean) =>
  invoke("set_deck_cue", { deck, active });
/** Headphone cue/master blend: 0 = cue bus only, 1 = master only. */
export const setCueMix = (value: number) => invoke("set_cue_mix", { value });
/** Headphone output level. */
export const setCueVolume = (value: number) => invoke("set_cue_volume", { value });

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
export const onEngineLoad = (cb: (e: EngineLoad) => void): Promise<UnlistenFn> =>
  listen<EngineLoad>("engine:load", (e) => cb(e.payload));
