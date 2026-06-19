/**
 * MIDI control mapping: the persisted binding table plus helpers shared by the mapping hook
 * and panel. A binding is just `sourceKey -> targetId`; the live registry of targets (with
 * their apply functions) is built in `useMidiMap` from the decks/crossfader.
 */

/** Stable key for a MIDI source. `cc:<controller>` for knobs/faders, `note:<note>` for pads/keys. */
export type MidiSourceKey = string;

export const ccKey = (controller: number): MidiSourceKey => `cc:${controller}`;
export const noteKey = (note: number): MidiSourceKey => `note:${note}`;

/** Human label for a source key, e.g. "CC 74" / "Note 36". */
export function sourceLabel(key: MidiSourceKey): string {
  const [kind, n] = key.split(":");
  return kind === "cc" ? `CC ${n}` : `Note ${n}`;
}

/** Target id for a per-deck control, e.g. `deck:0:filter`. */
export const deckTarget = (deck: number, control: string): string => `deck:${deck}:${control}`;
export const GLOBAL_XFADER = "global:crossfader";

/** `sourceKey -> targetId`. One source drives one target; one target may have many sources. */
export type MidiBindings = Record<MidiSourceKey, string>;

const STORE_KEY = "compas.midimap.v1";

export function loadBindings(): MidiBindings {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as MidiBindings) : {};
  } catch {
    return {};
  }
}

export function saveBindings(b: MidiBindings): void {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(b));
  } catch {
    // localStorage unavailable (private mode / quota) — mappings just won't persist.
  }
}

/**
 * Starter profile for the Akai MPK Mini MK3 (factory Program 1): the 8 knobs send CC 70–77
 * and the 8 pads (Bank A) send notes 36–43. These are the commonly-documented defaults — if a
 * unit differs, re-learn any control to rebind it. Knobs drive the two-deck mixer (EQ/filter);
 * the pads finger-drum the sampler — the natural split for an MPK.
 */
export function mpkMiniMk3Profile(): MidiBindings {
  return {
    // Knobs K1–K8 → deck A EQ/filter, deck B EQ/filter.
    [ccKey(70)]: deckTarget(0, "hi"),
    [ccKey(71)]: deckTarget(0, "mid"),
    [ccKey(72)]: deckTarget(0, "low"),
    [ccKey(73)]: deckTarget(0, "filter"),
    [ccKey(74)]: deckTarget(1, "hi"),
    [ccKey(75)]: deckTarget(1, "mid"),
    [ccKey(76)]: deckTarget(1, "low"),
    [ccKey(77)]: deckTarget(1, "filter"),
    // Pads 1–8 → sampler pads 1–8.
    [noteKey(36)]: "sampler:pad:0",
    [noteKey(37)]: "sampler:pad:1",
    [noteKey(38)]: "sampler:pad:2",
    [noteKey(39)]: "sampler:pad:3",
    [noteKey(40)]: "sampler:pad:4",
    [noteKey(41)]: "sampler:pad:5",
    [noteKey(42)]: "sampler:pad:6",
    [noteKey(43)]: "sampler:pad:7",
  };
}
