// Typed wrappers over Tauri commands + events for the compas engine.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ask, message, open, save } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";

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
  /** Per-bin `[low, mid, high]` band energy (0..1) for frequency-colored waveforms. */
  band_peaks: [number, number, number][];
  /** Loudness-normalization factor applied on load (1.0 = none). */
  replay_gain: number;
}

/** Convert a `[low, mid, high]` band-energy triple into a CSS rgb() string for waveform coloring
 *  (low→red, mid→green, high→blue), brightened so quiet bins stay visible. */
export const bandColor = ([low, mid, high]: [number, number, number]): string => {
  const max = Math.max(low, mid, high, 1e-4);
  const r = Math.round((low / max) * 255);
  const g = Math.round((mid / max) * 255);
  const b = Math.round((high / max) * 255);
  return `rgb(${r}, ${g}, ${b})`;
};

/** How a musical key is displayed: Camelot wheel codes (8A/7B) or standard names (C#m/D). */
export type KeyNotation = "camelot" | "musical";

/** Format a detected key for display in the chosen notation, falling back to "—" when unknown. */
export const formatKey = (
  camelot: string | null | undefined,
  name: string | null | undefined,
  notation: KeyNotation,
): string => (notation === "musical" ? name || camelot || "—" : camelot || "—");

const PITCH_CLASS: Record<string, number> = {
  C: 0, "C#": 1, D: 2, "D#": 3, E: 4, F: 5, "F#": 6, G: 7, "G#": 8, A: 9, "A#": 10, B: 11,
};

/** Pitch class (0–11) of a detected key name like "C#m" or "G" (mode suffix ignored); null if
 *  unknown. Matches the DSP `estimate_key` note names (sharps). */
export const pitchClassOf = (keyName: string | null | undefined): number | null => {
  if (!keyName) return null;
  const note = keyName.endsWith("m") ? keyName.slice(0, -1) : keyName;
  return note in PITCH_CLASS ? PITCH_CLASS[note] : null;
};

// Pitch-class → notation tables, mirroring the DSP side (`analysis.rs`: PITCH_NAMES /
// CAMELOT_MAJOR / CAMELOT_MINOR). Indexed by pitch class (C=0 .. B=11). Kept in lockstep so a
// transposed key reads exactly as the engine would have detected the shifted pitch.
const PITCH_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;
const CAMELOT_MAJOR = ["8B", "3B", "10B", "5B", "12B", "7B", "2B", "9B", "4B", "11B", "6B", "1B"] as const;
const CAMELOT_MINOR = ["5A", "12A", "7A", "2A", "9A", "4A", "11A", "6A", "1A", "8A", "3A", "10A"] as const;

/** Resolve a detected key to its pitch class (0–11) and mode, preferring the musical name and
 *  falling back to the Camelot code. Null when neither is recognized. */
const resolveKey = (
  camelot: string | null | undefined,
  name: string | null | undefined,
): { pc: number; minor: boolean } | null => {
  const namePc = pitchClassOf(name);
  if (namePc !== null) return { pc: namePc, minor: !!name && name.endsWith("m") };
  if (camelot) {
    const minor = camelot.endsWith("A");
    const pc = (minor ? CAMELOT_MINOR : CAMELOT_MAJOR).indexOf(camelot as never);
    if (pc >= 0) return { pc, minor };
  }
  return null;
};

/** Transpose a detected key by `semitones` (may be negative), returning the effective Camelot
 *  code and musical name — what the engine's pitch shift makes the deck sound like. Returns the
 *  key unchanged when the shift is 0 or the key is unknown. Mode (major/minor) is preserved. */
export const transposeKey = (
  camelot: string | null | undefined,
  name: string | null | undefined,
  semitones: number,
): { camelot: string | null; name: string | null } => {
  if (!semitones) return { camelot: camelot ?? null, name: name ?? null };
  const resolved = resolveKey(camelot, name);
  if (!resolved) return { camelot: camelot ?? null, name: name ?? null };
  const next = (((resolved.pc + semitones) % 12) + 12) % 12;
  return resolved.minor
    ? { camelot: CAMELOT_MINOR[next], name: `${PITCH_NAMES[next]}m` }
    : { camelot: CAMELOT_MAJOR[next], name: PITCH_NAMES[next] };
};

export interface DeckPosition {
  deck: number;
  frame: number;
  playing: boolean;
  level: number;
  /** Effective advance in source frames/sec — use to extrapolate the play-head between events. */
  rate: number;
  /** Measured output (DAC) latency in seconds. */
  latency_secs: number;
}

/** Extrapolate the visual play-head from the last telemetry sample so it scrolls smoothly at the
 *  display refresh rate (decoupled from the 30 Hz event rate and the audio buffer size), offset by
 *  the DAC latency so the marker matches what's being heard.
 *  `dtSeconds` is wall-clock time elapsed since the position event arrived. */
export const extrapolateFrame = (p: DeckPosition, dtSeconds: number): number => {
  if (!p.playing) return p.frame;
  return p.frame + p.rate * (dtSeconds - p.latency_secs);
};

export interface MasterMeter {
  l: number;
  r: number;
}

export interface EngineLoad {
  /** Audio-callback load, 0..~1 (≥1 = overrun). */
  load: number;
  /** Cumulative real-time-budget overruns since start. */
  xruns: number;
  /** Control messages dropped because the command ring was full. */
  command_ring_full: number;
  /** Master recording frames dropped because the record ring was full. */
  record_ring_drops: number;
  /** Headphone/cue frames dropped because the cue ring was full. */
  cue_ring_drops: number;
  /** Reclaim ring pressure events; retired buffers were parked for later off-thread drop. */
  reclaim_ring_full: number;
}

export interface DeckError {
  deck: number;
  message: string;
}

export interface EngineStatus {
  sample_rate: number;
  audio_online: boolean;
  audio_restarting: boolean;
  audio_restarts: number;
  audio_error: string | null;
  cue_device_latency_secs: number;
  cue_prime_latency_secs: number;
  booth_device_latency_secs: number;
  booth_prime_latency_secs: number;
  decks: { deck: number; loaded: boolean; playing: boolean; frame: number }[];
}

export type FilterMode = "off" | "lowpass" | "highpass";

export interface BuildInfo {
  /** `Cargo.toml` package version (e.g. `0.1.0`). */
  version: string;
  /** Short git SHA the binary was built from, or `"dev"` when built outside a checkout. */
  sha: string;
  /** Unix-seconds string captured at compile time, or empty string if unavailable. */
  built_at: string;
}

// ---- Commands ---------------------------------------------------------------------

export async function engineStatus(): Promise<EngineStatus> {
  return invoke<EngineStatus>("engine_status");
}

export async function buildInfo(): Promise<BuildInfo> {
  return invoke<BuildInfo>("build_info");
}

/** Export a diagnostics bundle (app/build info, engine status + RT telemetry, device list, library
 *  summary, and the given `settings`) to a `.zip` for bug reports — no audio. Opens a save dialog;
 *  resolves true when written, false if cancelled. */
export async function exportDiagnostics(settings: Record<string, unknown> = {}): Promise<boolean> {
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  const dest = await save({
    defaultPath: `compas-diagnostics-${stamp}.zip`,
    filters: [{ name: "Diagnostics bundle", extensions: ["zip"] }],
  });
  if (!dest) return false;
  await invoke("export_diagnostics", {
    generatedAt: new Date().toISOString(),
    settings,
    destPath: dest,
  });
  return true;
}

/**
 * Manual update check (fired from the title-bar version chip). Talks to the configured
 * GitHub Releases `latest.json` endpoint; if a newer signed build exists, prompts the
 * user before downloading, then offers to restart. Silent + safe to call repeatedly.
 *
 * Returns `true` only when an update was applied and the relaunch was queued — callers
 * use this to put the chip into a "restarting…" state. Any failure is surfaced via a
 * dialog and the function resolves to `false` so the chip returns to idle.
 */
export async function checkForUpdate(): Promise<boolean> {
  try {
    const upd = await checkUpdate();
    if (!upd) {
      await message("You're on the latest version.", { title: "compas — up to date", kind: "info" });
      return false;
    }
    const ok = await ask(
      `compas ${upd.version} is available (you're on ${upd.currentVersion}).\n\nDownload and install now? The app will restart.`,
      { title: "Update available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
    );
    if (!ok) return false;
    await upd.downloadAndInstall();
    await relaunch();
    return true;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    // Before the first published release (or if the release feed is briefly unreachable),
    // the updater can't fetch/parse `latest.json` and throws a parse/404/network error.
    // That isn't a real failure — there's simply nothing newer — so present it as "up to date"
    // rather than a scary raw error. Genuine unexpected errors still surface.
    const benign = /expected value|expected ident|EOF|decoding response|invalid|json|404|not found|could not fetch|fetch|network|dns|timed out|timeout|connection/i.test(msg);
    await message(
      benign ? "You're on the latest version." : `Update check failed:\n\n${msg}`,
      { title: benign ? "compas — up to date" : "compas — update check", kind: benign ? "info" : "error" },
    );
    return false;
  }
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

/** Open a native folder picker; returns the chosen directory or null. */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: true });
  return typeof selected === "string" ? selected : null;
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
  /** User tags (lowercased) — usable in `tag:` search and smart crates. */
  tags: string[];
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
/** Tag a track (lowercased, idempotent). */
export const dbAddTag = (path: string, tag: string) => invoke("db_add_tag", { path, tag });
export const dbRemoveTag = (path: string, tag: string) => invoke("db_remove_tag", { path, tag });

// ---- Watched folders (auto-import) ----
export const listWatchFolders = () => invoke<string[]>("list_watch_folders");
/** Register a folder + import its audio files now; returns the count newly imported. */
export const addWatchFolder = (path: string) => invoke<number>("add_watch_folder", { path });
export const removeWatchFolder = (path: string) => invoke("remove_watch_folder", { path });
/** Re-scan all watched folders for new files; returns the total newly imported. */
export const rescanWatchFolders = () => invoke<number>("rescan_watch_folders");
/** Fires after a watched-folder scan imports tracks — refresh the library view. */
export const onLibraryChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("library:changed", () => cb());
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
// ---- Library: search + crates/playlists -------------------------------------------
export interface DbCrate {
  id: number;
  name: string;
  is_playlist: boolean;
  track_count: number;
  /** True for a smart crate (populated by a saved search rather than manual membership). */
  is_smart: boolean;
}
/** Search with the grammar: `bpm:120-128 key:8A artist:foo title:bar -live · OR groups`. */
export const dbSearch = (query: string) => invoke<DbTrack[]>("db_search", { query });
export const dbCreateCrate = (name: string, isPlaylist: boolean) =>
  invoke<number>("db_create_crate", { name, isPlaylist });
/** Create a smart crate from a saved search query (populates dynamically). */
export const dbCreateSmartCrate = (name: string, query: string) =>
  invoke<number>("db_create_smart_crate", { name, query });
export const dbDeleteCrate = (id: number) => invoke("db_delete_crate", { id });
export const dbListCrates = () => invoke<DbCrate[]>("db_list_crates");
export const dbAddToCrate = (crateId: number, path: string) =>
  invoke("db_add_to_crate", { crateId, path });
export const dbRemoveFromCrate = (crateId: number, path: string) =>
  invoke("db_remove_from_crate", { crateId, path });
export const dbCrateTracks = (crateId: number) =>
  invoke<DbTrack[]>("db_crate_tracks", { crateId });

/** Counts of what `importCrate` wrote back into the library. */
export interface ImportSummary {
  tracks: number;
  cues: number;
  loops: number;
  tags: number;
  /** Id of the crate recreated on import. */
  crate_id: number | null;
}

/** Export a crate to a portable `.zip` package — the manifest (tracks + cues/loops/grids/key/tags)
 *  plus the crate's audio files, so a set moves between machines without the originals. Opens a save
 *  dialog; resolves true when written, false if the user cancelled. */
export async function exportCrate(crateId: number, crateName: string): Promise<boolean> {
  const safe = crateName.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "crate";
  const dest = await save({
    defaultPath: `${safe}.compas-crate.zip`,
    filters: [{ name: "Compás crate package", extensions: ["zip"] }],
  });
  if (!dest) return false;
  await invoke("export_crate_package", { crateId, destPath: dest });
  return true;
}

/** Import a crate written by `exportCrate` — a `.zip` package (audio extracted + relinked) or a
 *  `.json` manifest (tracks relinked by their stored path) — reading its performance data back into
 *  the library and recreating the crate. Opens a file picker; resolves the summary, or null if
 *  cancelled. */
export async function importCrate(): Promise<ImportSummary | null> {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Compás crate", extensions: ["zip", "json"] }],
  });
  if (typeof selected !== "string") return null;
  return invoke<ImportSummary>("import_crate", { srcPath: selected });
}

/** Auto-mix planner: ranked next-track suggestions (harmonic + tempo) after `currentPath`. */
export const dbPlanNext = (currentPath: string, limit: number) =>
  invoke<DbTrack[]>("db_plan_next", { currentPath, limit });

// ---- Controllers (mapping profiles) -----------------------------------------------
export interface ControllerBinding {
  channel: number;
  input:
    | { kind: "note"; note: number }
    | { kind: "cc"; cc: number }
    | { kind: "hid"; byte: number };
  control: string;
  soft_takeover?: boolean;
}
export interface ControllerProfile {
  id: string;
  name: string;
  ports?: { input?: string | null; output?: string | null };
  bindings: ControllerBinding[];
  script?: string | null;
}
/** A mappable control from the engine's control-bus registry (for the learn editor). */
export interface ControlSpec {
  id: string;
  label: string;
  unit: string;
  behavior: { kind: string; min?: number; max?: number; steps?: number };
}
/** The full list of mappable controls. */
export const controllerRegistry = () => invoke<ControlSpec[]>("controller_registry");
/** List controller profiles in the user controller directory. */
export const controllerList = () => invoke<ControllerProfile[]>("controller_list");
/** Save (or overwrite) a controller profile. */
export const controllerSave = (profile: ControllerProfile) => invoke("controller_save", { profile });

/** Export controller profiles to a shareable `.compas-profiles.json` pack. `ids` selects which of
 *  the merged (bundled + user) profiles to include; an empty list exports them all. Opens a save
 *  dialog; resolves the number written, or null if cancelled. */
export async function exportProfilePack(ids: string[] = []): Promise<number | null> {
  const dest = await save({
    defaultPath: "controllers.compas-profiles.json",
    filters: [{ name: "Compás controller pack", extensions: ["json"] }],
  });
  if (!dest) return null;
  return invoke<number>("export_profile_pack", { ids, destPath: dest });
}

/** Import a controller profile pack into the user directory (overwriting profiles with the same
 *  id). Opens a file picker; resolves the imported profile ids, or null if cancelled. */
export async function importProfilePack(): Promise<string[] | null> {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Compás controller pack", extensions: ["json"] }],
  });
  if (typeof selected !== "string") return null;
  return invoke<string[]>("import_profile_pack", { srcPath: selected });
}
/** Activate a profile (its bindings + script resolve incoming MIDI to controller:update events). */
export const controllerActivate = (profile: ControllerProfile) =>
  invoke("controller_activate", { profile });
export const controllerDeactivate = () => invoke("controller_deactivate");
/** Reflect a control's current engine value back onto the device (LEDs, motor faders). No-op
 *  unless a profile + output port are active; engine-side dedup skips redundant resends. */
export const controllerFeedback = (control: string, value: number) =>
  invoke("controller_feedback", { control, value });

/** A resolved control change from the active controller profile: control-bus id + engine value. */
export interface ControllerUpdate {
  control: string;
  value: number;
}
export const onControllerUpdate = (cb: (u: ControllerUpdate) => void): Promise<UnlistenFn> =>
  listen<ControllerUpdate>("controller:update", (e) => cb(e.payload));

// ---- HID controllers (non-class-compliant: NI Traktor, etc.) ---------------------
export interface HidDeviceInfo {
  path: string;
  vendor_id: number;
  product_id: number;
  manufacturer: string;
  product: string;
}
/** A changed byte in an HID input report (for the learn editor). */
export interface HidInput {
  byte: number;
  value: number;
}
/** List connected HID devices. */
export const hidList = () => invoke<HidDeviceInfo[]>("hid_list");
/** Open an HID device by path; its changed report bytes drive the active profile's `hid` bindings. */
export const hidConnect = (path: string) => invoke("hid_connect", { path });
/** Close the active HID connection. */
export const hidDisconnect = () => invoke("hid_disconnect");
export const onHidInput = (cb: (i: HidInput) => void): Promise<UnlistenFn> =>
  listen<HidInput>("hid:input", (e) => cb(e.payload));

export const loadTrack = (deck: number, path: string) => invoke("load_track", { deck, path });
export const deckPlay = (deck: number) => invoke("deck_play", { deck });
export const deckPause = (deck: number) => invoke("deck_pause", { deck });
export const deckSeek = (deck: number, frame: number) => invoke("deck_seek", { deck, frame });
export const deckUnload = (deck: number) => invoke("deck_unload", { deck });
export const setLoop = (deck: number, inFrame: number, outFrame: number, active: boolean) =>
  invoke("set_loop", { deck, inFrame, outFrame, active });
export const setLoopActive = (deck: number, active: boolean) =>
  invoke("set_loop_active", { deck, active });
/** Momentary loop-roll with slip. Engage with a grid-snapped region; release (active=false)
 *  drops back in where the track would be. */
export const setLoopRoll = (
  deck: number,
  inFrame: number,
  outFrame: number,
  active: boolean,
) => invoke("set_loop_roll", { deck, inFrame, outFrame, active });
/** Jog-wheel scratch: speed 1.0 = natural play rate, negative = reverse, 0 = held. */
export const deckScratch = (deck: number, active: boolean, speed: number) =>
  invoke("deck_scratch", { deck, active, speed });
export const setDeckTempo = (deck: number, ratio: number) =>
  invoke("set_deck_tempo", { deck, ratio });
/** Key-lock (master tempo): tempo changes preserve pitch. */
export const setDeckKeylock = (deck: number, active: boolean) =>
  invoke("set_deck_keylock", { deck, active });
/** Key shift: transpose a deck by ± semitones without changing tempo (WSOLA pitch shift). */
export const setDeckPitchShift = (deck: number, semitones: number) =>
  invoke("set_deck_pitch_shift", { deck, semitones });
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
/** Flanger insert. `rateHz` LFO sweep, `depth` 0..1 sweep width, plus feedback + wet `mix`. */
export const setDeckFlanger = (
  deck: number,
  active: boolean,
  rateHz: number,
  depth: number,
  feedback: number,
  mix: number,
) => invoke("set_deck_flanger", { deck, active, rateHz, depth, feedback, mix });
/** Bitcrusher insert. `bits` 1..16 quantisation, `downsample` 1..64 sample-and-hold, wet `mix`. */
export const setDeckCrusher = (
  deck: number,
  active: boolean,
  bits: number,
  downsample: number,
  mix: number,
) => invoke("set_deck_crusher", { deck, active, bits, downsample, mix });
export const setCrossfader = (value: number) => invoke("set_crossfader", { value });
/** Crossfader response: `curve` (steepness, >=0.25), `mode` (0 = constant-power, 1 = additive/cut), `reverse`. */
export const setCrossfaderConfig = (curve: number, mode: 0 | 1, reverse: boolean) =>
  invoke("set_crossfader_config", { curve, mode, reverse });
// ---- Main CUE button (CDJ / gated) ------------------------------------------------
/** Cue button behavior: 0 = CDJ (preview-while-held + set-when-paused), 1 = gated/stutter. */
export const setCueMode = (deck: number, mode: 0 | 1) => invoke("set_cue_mode", { deck, mode });
export const setCuePoint = (deck: number, frame: number) => invoke("set_cue_point", { deck, frame });
export const cueButton = (deck: number, pressed: boolean) =>
  invoke("cue_button", { deck, pressed });
// ---- Loop toolkit -----------------------------------------------------------------
/** Scale the active loop length: 0.5 halves, 2.0 doubles (anchored at loop-in). */
export const scaleLoop = (deck: number, factor: number) => invoke("scale_loop", { deck, factor });
/** Shift the loop region (and play-head) by `deltaFrames`. */
export const moveLoop = (deck: number, deltaFrames: number) =>
  invoke("move_loop", { deck, deltaFrames });
// ---- Sync coordinator -------------------------------------------------------------
/** Follower sync mode: 0 = full tempo+phase, 1 = tempo-only. */
export const setDeckSyncMode = (deck: number, mode: 0 | 1) =>
  invoke("set_deck_sync_mode", { deck, mode });
/** Pin/unpin a deck as the explicit sync leader. */
export const setSyncLeader = (deck: number, explicit: boolean) =>
  invoke("set_sync_leader", { deck, explicit });
/** Auto-pick the best leader and follow it. */
export const syncToLeader = (deck: number) => invoke("sync_to_leader", { deck });
// ---- Loudness normalization (ReplayGain) ------------------------------------------
/** Override a deck's loudness-normalization factor (1.0 = off). Auto-applied on load. */
export const setDeckReplayGain = (deck: number, gain: number) =>
  invoke("set_deck_replay_gain", { deck, gain });
// ---- FX macro (super-knob) --------------------------------------------------------
/** Drive a deck's FX macro: one knob (0..1) brings in reverb then echo across the sweep. */
export const setDeckFxMacro = (deck: number, value: number) =>
  invoke("set_deck_fx_macro", { deck, value });
export const setMasterGain = (value: number) => invoke("set_master_gain", { value });

// ---- Synth instrument + MIDI ------------------------------------------------------

export const noteOn = (note: number, velocity: number) => invoke("note_on", { note, velocity });
export const noteOff = (note: number) => invoke("note_off", { note });
export const allNotesOff = () => invoke("all_notes_off");
export const setSynthWaveform = (index: number) => invoke("set_synth_waveform", { index });
export const setSynthGain = (gain: number) => invoke("set_synth_gain", { gain });

// ---- Sampler / performance pads ---------------------------------------------------

export interface LoadedSample {
  slot: number;
  name: string;
}
/** Number of sampler pads (engine-defined). */
export const samplerPadCount = () => invoke<number>("sampler_pad_count");
/** Decode an audio file into a pad; returns the pad index + display name. */
export const loadSample = (slot: number, path: string) =>
  invoke<LoadedSample>("load_sample", { slot, path });
export const clearSample = (slot: number) => invoke("clear_sample", { slot });
/** Trigger a pad (velocity 0..127). One-shot pads overlap; looped pads toggle. */
export const triggerSample = (slot: number, velocity = 110) =>
  invoke("trigger_sample", { slot, velocity });
export const stopSample = (slot: number) => invoke("stop_sample", { slot });
export const setSampleLoop = (slot: number, looping: boolean) =>
  invoke("set_sample_loop", { slot, looping });
export const setSamplerGain = (gain: number) => invoke("set_sampler_gain", { gain });
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
  channel: number;
  controller: number;
  value: number;
}
export const onMidiCc = (cb: (e: MidiCc) => void): Promise<UnlistenFn> =>
  listen<MidiCc>("midi:cc", (e) => cb(e.payload));

export interface MidiNote {
  channel: number;
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
/** Start booth monitoring on `device` (omit for the default output); returns the device name. */
export const startBoothOutput = (device?: string) =>
  invoke<string>("start_booth_output", { device: device ?? null });
export const stopBoothOutput = () => invoke("stop_booth_output");
/** Booth output level, fed from the post-master mix. */
export const setBoothVolume = (value: number) => invoke("set_booth_volume", { value });

/** List input devices (mics / line-in) the aux input can capture from. */
export const listInputDevices = () => invoke<string[]>("list_input_devices");
/** Start aux/mic capture on `device` (omit for the default input); returns the device name. */
export const startAuxInput = (device?: string) =>
  invoke<string>("start_aux_input", { device: device ?? null });
export const stopAuxInput = () => invoke("stop_aux_input");
/** Aux/mic input level, summed into the master bus. */
export const setAuxGain = (value: number) => invoke("set_aux_gain", { value });

/** Live beat-tracker readout for the aux input. */
export interface LiveBeat {
  active: boolean;
  bpm: number;
  beat_phase: number;
  confidence: number;
  locked: boolean;
}
/** Current live beat-tracker readout (tempo/phase/confidence/lock of the aux input). */
export const liveBeatClock = () => invoke<LiveBeat>("live_beat_clock");
/** Make a deck tempo-match the live beat clock (mic/aux), or stop. */
export const setDeckSyncLive = (deck: number, active: boolean) =>
  invoke("set_deck_sync_live", { deck, active });
/** Set the internal master clock's tempo and whether it runs as a virtual sync leader. */
export const setInternalClock = (active: boolean, bpm: number) =>
  invoke("set_internal_clock", { active, bpm });
/** Make a deck tempo/phase-match the internal master clock, or stop. */
export const setDeckSyncInternal = (deck: number, active: boolean) =>
  invoke("set_deck_sync_internal", { deck, active });

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
