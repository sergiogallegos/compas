import { useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  deckPause,
  deckPlay,
  deckScratch,
  deckSeek,
  deckUnload,
  setDeckKeylock,
  inTauri,
  loadTrack,
  onDeckError,
  onDeckLoaded,
  onDeckLoading,
  onDeckPosition,
  pickAudioFile,
  setDeckEcho,
  setDeckReverb,
  setDeckEq,
  setDeckFilter,
  setDeckGain,
  setDeckTempo,
  setLoop as setLoopCmd,
  setLoopActive as setLoopActiveCmd,
  type DeckLoaded,
  type FilterMode,
} from "../lib/ipc";

export interface LoopState {
  active: boolean;
  /** Beat length of the active beat-loop, or null for a manual in/out loop. */
  beats: number | null;
  inFrame: number;
  outFrame: number;
}

/** Tempo step per −/+ trim click (0.1%). */
const TEMPO_TRIM = 0.001;
/** Tempo trim range (±10%), keeping the pitch fader meaningful. */
const TEMPO_TRIM_MIN = 0.9;
const TEMPO_TRIM_MAX = 1.1;

export interface Eq {
  hi: number;
  mid: number;
  low: number;
}

export interface EchoState {
  active: boolean;
  /** Delay time as a fraction/multiple of a beat (¼, ½, 1, 2). */
  beats: number;
  /** Single 0..1 "amount" knob driving both feedback and wet mix. */
  depth: number;
}

export interface ReverbState {
  active: boolean;
  /** Room size 0..1 (tail length). */
  size: number;
  /** Wet/dry mix 0..1. */
  mix: number;
}

/** Fallback echo time (seconds) for one beat when a track has no beatgrid. */
const NO_GRID_BEAT_SEC = 0.5;

export interface DeckState {
  meta: DeckLoaded | null;
  frame: number;
  playing: boolean;
  level: number;
  tempo: number;
  /** Key-lock (master tempo): tempo changes preserve pitch. */
  keylock: boolean;
  eq: Eq;
  filter: number;
  gain: number;
  hotCues: (number | null)[];
  loop: LoopState;
  echo: EchoState;
  reverb: ReverbState;
  error: string | null;
  /** True between clicking load and the track being decoded/analyzed. */
  loading: boolean;
  /** Whether this deck supports full DSP (local) vs control-only (streaming). */
  dsp: boolean;
}

function filterParams(x: number): { mode: FilterMode; cutoff: number; resonance: number } {
  if (Math.abs(x) < 0.02) return { mode: "off", cutoff: 1000, resonance: 0.9 };
  if (x < 0) return { mode: "lowpass", cutoff: 20000 * Math.pow(200 / 20000, -x), resonance: 0.9 + -x };
  return { mode: "highpass", cutoff: 20 * Math.pow(4000 / 20, x), resonance: 0.9 + x };
}

export function useDeck(deck: number, dsp = true) {
  const [meta, setMeta] = useState<DeckLoaded | null>(null);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [level, setLevel] = useState(0);
  const [tempo, setTempo] = useState(1);
  const [keylock, setKeylockState] = useState(false);
  const [eq, setEqState] = useState<Eq>({ hi: 0, mid: 0, low: 0 });
  const [filter, setFilterState] = useState(0);
  const [gain, setGainState] = useState(1);
  const [hotCues, setHotCues] = useState<(number | null)[]>(Array(8).fill(null));
  const [loop, setLoopState] = useState<LoopState>({ active: false, beats: null, inFrame: 0, outFrame: 0 });
  const [echo, setEchoState] = useState<EchoState>({ active: false, beats: 0.5, depth: 0.5 });
  const [reverb, setReverbState] = useState<ReverbState>({ active: false, size: 0.6, mix: 0.3 });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const frameRef = useRef(0);
  const tempoRef = useRef(tempo);
  tempoRef.current = tempo;
  const keylockRef = useRef(keylock);
  keylockRef.current = keylock;
  const loopRef = useRef(loop);
  loopRef.current = loop;
  const echoRef = useRef(echo);
  echoRef.current = echo;
  const reverbRef = useRef(reverb);
  reverbRef.current = reverb;

  useEffect(() => {
    if (!inTauri()) return;
    const unlistens: UnlistenFn[] = [];
    let active = true;
    const track = (p: Promise<UnlistenFn>) => p.then((u) => (active ? unlistens.push(u) : u()));

    track(
      onDeckLoading((e) => {
        if (e.deck !== deck) return;
        setLoading(true);
        setError(null);
      }),
    );
    track(
      onDeckLoaded((e) => {
        if (e.deck !== deck) return;
        setMeta(e);
        setFrame(0);
        frameRef.current = 0;
        setPlaying(false);
        setTempo(1);
        setKeylockState(false); // engine resets key-lock on load
        setHotCues(Array(8).fill(null));
        setLoopState({ active: false, beats: null, inFrame: 0, outFrame: 0 });
        // Engine resets FX on load; mirror that (keep the user's param settings).
        setEchoState((e) => ({ ...e, active: false }));
        setReverbState((r) => ({ ...r, active: false }));
        setError(null);
        setLoading(false);
      }),
    );
    track(
      onDeckPosition((e) => {
        if (e.deck !== deck) return;
        setFrame(e.frame);
        frameRef.current = e.frame;
        setPlaying(e.playing);
        setLevel(e.level);
      }),
    );
    track(
      onDeckError((e) => {
        if (e.deck !== deck) return;
        setError(e.message);
        setLoading(false);
      }),
    );

    return () => {
      active = false;
      unlistens.forEach((u) => u());
    };
  }, [deck]);

  const actions = useMemo(() => {
    const swallow = () => {};
    // Translate the UI echo (beat-synced time + single "depth") to engine params.
    const pushEcho = (e: EchoState) => {
      const beatSec = (meta?.beat_interval_sec ?? 0) > 0 ? meta!.beat_interval_sec : NO_GRID_BEAT_SEC;
      const timeSec = beatSec * e.beats;
      const feedback = 0.15 + e.depth * 0.7; // 0.15..0.85
      const mix = e.active ? e.depth * 0.5 : 0; // up to half-wet
      setDeckEcho(deck, e.active, timeSec, feedback, mix).catch(swallow);
    };
    const pushReverb = (r: ReverbState) => {
      setDeckReverb(deck, r.active, r.size, r.active ? r.mix : 0).catch(swallow);
    };
    return {
      load: async () => {
        try {
          const path = await pickAudioFile();
          if (path) await loadTrack(deck, path);
        } catch (e) {
          setError(String(e));
        }
      },
      eject: () => {
        deckUnload(deck).catch(swallow);
        setMeta(null);
        setPlaying(false);
      },
      togglePlay: () => (playing ? deckPause(deck) : deckPlay(deck)).catch(swallow),
      cue: () => deckSeek(deck, 0).catch(swallow),
      seekFrac: (f: number) => {
        const frames = meta?.frames ?? 0;
        deckSeek(deck, f * frames).catch(swallow);
      },
      // Jog-wheel scratch: the platter streams a read-rate from drag velocity.
      scratch: (active: boolean, speed: number) => deckScratch(deck, active, speed).catch(swallow),
      setTempo: (ratio: number) => {
        setTempo(ratio);
        setDeckTempo(deck, ratio).catch(swallow);
      },
      toggleKeylock: () => {
        const next = !keylockRef.current;
        setKeylockState(next);
        setDeckKeylock(deck, next).catch(swallow);
      },
      // Persistent fine tempo trim (the jog wheel handles momentary pitch bend). Reads
      // the ref so rapid clicks accumulate instead of all seeing the same render's tempo.
      trimTempo: (dir: 1 | -1) => {
        const next = Math.min(TEMPO_TRIM_MAX, Math.max(TEMPO_TRIM_MIN, tempoRef.current + dir * TEMPO_TRIM));
        tempoRef.current = next;
        setTempo(next);
        setDeckTempo(deck, next).catch(swallow);
      },
      setEq: (next: Eq) => {
        setEqState(next);
        setDeckEq(deck, next.low, next.mid, next.hi).catch(swallow);
      },
      setFilter: (x: number) => {
        setFilterState(x);
        const p = filterParams(x);
        setDeckFilter(deck, p.mode, p.cutoff, p.resonance).catch(swallow);
      },
      setGain: (g: number) => {
        setGainState(g);
        setDeckGain(deck, g).catch(swallow);
      },
      setHotCue: (i: number) => {
        setHotCues((cur) => {
          const next = [...cur];
          // Set if empty, else jump to it.
          if (next[i] == null) next[i] = frameRef.current;
          else deckSeek(deck, next[i] as number).catch(swallow);
          return next;
        });
      },
      clearHotCue: (i: number) => {
        setHotCues((cur) => {
          const next = [...cur];
          next[i] = null;
          return next;
        });
      },
      beatLoop: (beats: number) => {
        if (!meta || (meta.beat_interval_sec ?? 0) <= 0) return;
        const sr = meta.source_rate;
        const interval = meta.beat_interval_sec * sr;
        const offset = meta.first_beat_sec * sr;
        const k = Math.round((frameRef.current - offset) / interval);
        const inFrame = Math.max(0, offset + k * interval);
        const outFrame = inFrame + beats * interval;
        const cur = loopRef.current;
        if (cur.active && cur.beats === beats) {
          setLoopActiveCmd(deck, false).catch(swallow);
          setLoopState({ ...cur, active: false });
        } else {
          setLoopCmd(deck, inFrame, outFrame, true).catch(swallow);
          setLoopState({ active: true, beats, inFrame, outFrame });
        }
      },
      loopIn: () => setLoopState((l) => ({ ...l, inFrame: frameRef.current, beats: null })),
      loopOut: () => {
        const out = frameRef.current;
        const l = loopRef.current;
        if (out > l.inFrame) {
          setLoopCmd(deck, l.inFrame, out, true).catch(swallow);
          setLoopState({ active: true, beats: null, inFrame: l.inFrame, outFrame: out });
        }
      },
      clearLoop: () => {
        setLoopActiveCmd(deck, false).catch(swallow);
        setLoopState((l) => ({ ...l, active: false }));
      },
      toggleEcho: () => {
        const next = { ...echoRef.current, active: !echoRef.current.active };
        setEchoState(next);
        pushEcho(next);
      },
      setEchoBeats: (beats: number) => {
        const next = { ...echoRef.current, beats };
        setEchoState(next);
        if (next.active) pushEcho(next);
      },
      setEchoDepth: (depth: number) => {
        const next = { ...echoRef.current, depth };
        setEchoState(next);
        if (next.active) pushEcho(next);
      },
      toggleReverb: () => {
        const next = { ...reverbRef.current, active: !reverbRef.current.active };
        setReverbState(next);
        pushReverb(next);
      },
      setReverbSize: (size: number) => {
        const next = { ...reverbRef.current, size };
        setReverbState(next);
        if (next.active) pushReverb(next);
      },
      setReverbMix: (mix: number) => {
        const next = { ...reverbRef.current, mix };
        setReverbState(next);
        if (next.active) pushReverb(next);
      },
    };
  }, [deck, playing, tempo, meta]);

  const state: DeckState = { meta, frame, playing, level, tempo, keylock, eq, filter, gain, hotCues, loop, echo, reverb, error, loading, dsp };
  return { state, actions };
}

export type DeckController = ReturnType<typeof useDeck>;
