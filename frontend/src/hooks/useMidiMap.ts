import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { inTauri, onMidiCc, onMidiNote } from "../lib/ipc";
import type { DeckController } from "./useDeck";
import {
  ccKey,
  deckTarget,
  GLOBAL_XFADER,
  loadBindings,
  type MidiBindings,
  type MidiSourceKey,
  noteKey,
  saveBindings,
} from "../lib/midiMap";

type TargetKind = "continuous" | "trigger";

export interface MidiTarget {
  id: string;
  label: string;
  group: string;
  kind: TargetKind;
  /** Continuous-only range the 0..127 CC value maps onto. */
  min?: number;
  max?: number;
  /** continuous: receives the scaled value; trigger: called once per press. */
  apply: (v: number) => void;
}

const DECK_LETTERS = ["A", "B", "C", "D"];
/** Sampler pad count exposed as mapping targets (matches `compas-audio` NUM_SAMPLER_PADS). */
const SAMPLER_PADS = 8;

/** App-level handlers the per-deck registry can't express on its own. */
export interface MidiMapExtras {
  crossfader: (v: number) => void;
  /** Toggle continuous beat-sync for a deck against its on-screen partner. */
  syncDeck: (deck: number) => void;
  /** Toggle headphone pre-fader-listen for a deck. */
  deckCue: (deck: number) => void;
  /** Fire a sampler pad (0-based). */
  samplerPad: (pad: number) => void;
}

function buildTargets(decks: DeckController[], extra: MidiMapExtras): MidiTarget[] {
  const out: MidiTarget[] = [];
  decks.forEach((d, i) => {
    const g = `Deck ${DECK_LETTERS[i]}`;
    const a = d.actions;
    const cont = (control: string, label: string, min: number, max: number, apply: (v: number) => void) =>
      out.push({ id: deckTarget(i, control), label, group: g, kind: "continuous", min, max, apply });
    const trig = (control: string, label: string, apply: () => void) =>
      out.push({ id: deckTarget(i, control), label, group: g, kind: "trigger", apply });

    cont("gain", "Gain", 0, 1.5, (v) => a.setGain(v));
    cont("hi", "EQ Hi", -26, 6, (v) => a.setEq({ ...d.state.eq, hi: v }));
    cont("mid", "EQ Mid", -26, 6, (v) => a.setEq({ ...d.state.eq, mid: v }));
    cont("low", "EQ Low", -26, 6, (v) => a.setEq({ ...d.state.eq, low: v }));
    cont("filter", "Filter", -1, 1, (v) => a.setFilter(v));
    cont("tempo", "Tempo", 0.9, 1.1, (v) => a.setTempo(v));

    trig("play", "Play / Pause", () => a.togglePlay());
    trig("cue", "Cue (to start)", () => a.cue());
    trig("pfl", "Headphone CUE", () => extra.deckCue(i));
    trig("sync", "Sync", () => extra.syncDeck(i));
    trig("keylock", "Key-lock", () => a.toggleKeylock());
    for (let c = 0; c < 4; c++) trig(`hotcue${c + 1}`, `Hot cue ${c + 1}`, () => a.setHotCue(c));
    for (const b of [4, 8, 16]) trig(`beatloop${b}`, `Loop ${b}`, () => a.beatLoop(b));
    trig("loopclear", "Loop off", () => a.clearLoop());
    trig("echo", "Echo", () => a.toggleEcho());
    trig("reverb", "Reverb", () => a.toggleReverb());
  });
  // Sampler pads — trigger one per pad (great for a controller's drum pads).
  for (let p = 0; p < SAMPLER_PADS; p++) {
    out.push({
      id: `sampler:pad:${p}`,
      label: `Pad ${p + 1}`,
      group: "Sampler",
      kind: "trigger",
      apply: () => extra.samplerPad(p),
    });
  }
  out.push({
    id: GLOBAL_XFADER,
    label: "Crossfader",
    group: "Global",
    kind: "continuous",
    min: 0,
    max: 1,
    apply: (v) => extra.crossfader(v),
  });
  return out;
}

export interface MidiMapApi {
  targets: MidiTarget[];
  bindings: MidiBindings;
  /** Target id currently in learn mode (awaiting the next MIDI source), or null. */
  learning: string | null;
  startLearn: (targetId: string) => void;
  cancelLearn: () => void;
  /** Remove every source bound to a target. */
  clearTarget: (targetId: string) => void;
  /** Source keys bound to a target (usually 0 or 1). */
  sourcesFor: (targetId: string) => MidiSourceKey[];
  setBindings: (b: MidiBindings) => void;
  clearAll: () => void;
  /** Last source seen, for the "listening…" hint and activity feedback. */
  lastSource: MidiSourceKey | null;
}

export function useMidiMap(decks: DeckController[], extra: MidiMapExtras): MidiMapApi {
  const targets = useMemo(() => buildTargets(decks, extra), [decks, extra]);
  const targetsById = useMemo(() => new Map(targets.map((t) => [t.id, t])), [targets]);
  const [bindings, setBindingsState] = useState<MidiBindings>(() => loadBindings());
  const [learning, setLearning] = useState<string | null>(null);
  const [lastSource, setLastSource] = useState<MidiSourceKey | null>(null);

  // Refs so the (once-subscribed) event handler always sees current state.
  const targetsRef = useRef(targetsById);
  targetsRef.current = targetsById;
  const bindingsRef = useRef(bindings);
  bindingsRef.current = bindings;
  const learningRef = useRef(learning);
  learningRef.current = learning;
  // Last CC value per source, for rising-edge detection when a knob drives a trigger.
  const lastCc = useRef<Map<MidiSourceKey, number>>(new Map());

  const commit = useCallback((b: MidiBindings) => {
    setBindingsState(b);
    saveBindings(b);
  }, []);

  /** Bind a source to the learning target, replacing any prior use of that source. */
  const bind = useCallback(
    (source: MidiSourceKey) => {
      const target = learningRef.current;
      if (!target) return;
      const next: MidiBindings = { ...bindingsRef.current, [source]: target };
      setLearning(null);
      commit(next);
    },
    [commit],
  );

  const dispatch = useCallback((source: MidiSourceKey, value: number, isTrigger: boolean) => {
    const targetId = bindingsRef.current[source];
    if (!targetId) return;
    const t = targetsRef.current.get(targetId);
    if (!t) return;
    if (t.kind === "continuous") {
      if (isTrigger) {
        // A pad bound to a continuous control: treat a press as snap-to-max.
        t.apply(t.max ?? 1);
      } else {
        const min = t.min ?? 0;
        const max = t.max ?? 1;
        t.apply(min + (value / 127) * (max - min));
      }
    } else if (isTrigger) {
      t.apply(0);
    }
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    const unlistens: UnlistenFn[] = [];
    let active = true;
    const track = (p: Promise<UnlistenFn>) => p.then((u) => (active ? unlistens.push(u) : u()));

    track(
      onMidiCc((e) => {
        const key = ccKey(e.controller);
        setLastSource(key);
        if (learningRef.current) {
          bind(key);
          return;
        }
        const prev = lastCc.current.get(key) ?? 0;
        lastCc.current.set(key, e.value);
        // Continuous knob: stream the value. Trigger bound to a knob: fire on the rising edge.
        const targetId = bindingsRef.current[key];
        const t = targetId ? targetsRef.current.get(targetId) : undefined;
        if (t?.kind === "trigger") {
          if (prev < 64 && e.value >= 64) dispatch(key, e.value, true);
        } else {
          dispatch(key, e.value, false);
        }
      }),
    );
    track(
      onMidiNote((e) => {
        if (!e.on) return; // act on press, ignore release
        const key = noteKey(e.note);
        setLastSource(key);
        if (learningRef.current) {
          bind(key);
          return;
        }
        dispatch(key, e.velocity, true);
      }),
    );

    return () => {
      active = false;
      unlistens.forEach((u) => u());
    };
  }, [bind, dispatch]);

  const sourcesFor = useCallback(
    (targetId: string) => Object.keys(bindings).filter((k) => bindings[k] === targetId),
    [bindings],
  );
  const clearTarget = useCallback(
    (targetId: string) => {
      const next: MidiBindings = {};
      for (const [k, v] of Object.entries(bindingsRef.current)) if (v !== targetId) next[k] = v;
      commit(next);
    },
    [commit],
  );
  const clearAll = useCallback(() => commit({}), [commit]);

  return {
    targets,
    bindings,
    learning,
    startLearn: setLearning,
    cancelLearn: () => setLearning(null),
    clearTarget,
    sourcesFor,
    setBindings: commit,
    clearAll,
    lastSource,
  };
}
