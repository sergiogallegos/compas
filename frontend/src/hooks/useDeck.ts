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
  setBeatgrid,
  setDeckSync,
  setDeckXfaderAssign,
  setDeckEcho,
  setDeckReverb,
  setDeckEq,
  setDeckFilter,
  setDeckGain,
  setDeckTempo,
  setLoop as setLoopCmd,
  setLoopActive as setLoopActiveCmd,
  setLoopRoll as setLoopRollCmd,
  dbClearCue,
  dbClearLoop,
  dbRecordPlay,
  dbSetCue,
  dbSetGain,
  dbSetGridOffset,
  dbSetLoop,
  dbTrackState,
  dbUpsertAnalysis,
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
  /** Manual beatgrid nudge (seconds) added to the analyzed first-beat anchor. */
  gridOffset: number;
  /** True while this deck is a continuous sync follower. */
  synced: boolean;
  /** Quantize: snap hot-cue jumps and beat-jumps to the beatgrid. */
  quantize: boolean;
  /** Crossfader routing: 0 = A side, 1 = thru, 2 = B side. */
  xfaderAssign: number;
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
  const [gridOffset, setGridOffset] = useState(0);
  const [synced, setSyncedState] = useState(false);
  const [quantize, setQuantize] = useState(false);
  // Default routing: deck 0 → A, deck 1 → B, decks C/D → through.
  const [xfaderAssign, setXfaderAssignState] = useState(deck === 0 ? 0 : deck === 1 ? 2 : 1);
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
  const gridOffsetRef = useRef(gridOffset);
  gridOffsetRef.current = gridOffset;
  const loopRef = useRef(loop);
  loopRef.current = loop;
  const echoRef = useRef(echo);
  echoRef.current = echo;
  const reverbRef = useRef(reverb);
  reverbRef.current = reverb;
  // Path of the loaded track + a once-per-load guard for recording a play.
  const pathRef = useRef<string | null>(null);
  const playedRef = useRef(false);
  const quantizeRef = useRef(quantize);
  quantizeRef.current = quantize;

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
        setGridOffset(0);
        setSyncedState(false); // engine resets sync on load
        setHotCues(Array(8).fill(null));
        setLoopState({ active: false, beats: null, inFrame: 0, outFrame: 0 });
        // Engine resets FX on load; mirror that (keep the user's param settings).
        setEchoState((e) => ({ ...e, active: false }));
        setReverbState((r) => ({ ...r, active: false }));
        setError(null);
        setLoading(false);
        pathRef.current = e.path;
        playedRef.current = false;

        if (dsp) {
          // Cache analysis onto the library row, then restore saved cues/loops/grid/gain.
          dbUpsertAnalysis(e).catch(() => {});
          dbTrackState(e.path)
            .then((st) => {
              if (pathRef.current !== e.path) return; // a newer track loaded meanwhile
              if (st.grid_offset_sec) {
                setGridOffset(st.grid_offset_sec);
                if ((e.beat_interval_sec ?? 0) > 0) {
                  const sr = e.source_rate;
                  setBeatgrid(
                    deck,
                    (e.first_beat_sec + st.grid_offset_sec) * sr,
                    e.beat_interval_sec * sr,
                  ).catch(() => {});
                }
              }
              if (st.gain !== 1) {
                setGainState(st.gain);
                setDeckGain(deck, st.gain).catch(() => {});
              }
              if (st.cues.length) {
                const arr: (number | null)[] = Array(8).fill(null);
                for (const c of st.cues) if (c.slot >= 0 && c.slot < 8) arr[c.slot] = c.frame;
                setHotCues(arr);
              }
              // Restore the saved loop region, but leave it disarmed (don't jump the play-head).
              const lp = st.loops.find((l) => l.slot === 0);
              if (lp) {
                setLoopState({
                  active: false,
                  beats: lp.beats,
                  inFrame: lp.in_frame,
                  outFrame: lp.out_frame,
                });
              }
            })
            .catch(() => {});
        }
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
    // Per-track persistence is keyed by file path and only for local (DSP) decks.
    const path = meta?.path ?? null;
    const canPersist = dsp && !!path;
    // Record a play once, the first time this loaded track starts.
    const markPlayed = () => {
      if (canPersist && !playedRef.current) {
        playedRef.current = true;
        dbRecordPlay(path as string).catch(swallow);
      }
    };
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
    // Current beatgrid in source frames (interval + first-beat offset incl. manual nudge), or
    // null when the track has no grid. Read live so a grid nudge takes effect immediately.
    const grid = (): { interval: number; offset: number } | null => {
      if (!meta || (meta.beat_interval_sec ?? 0) <= 0) return null;
      const sr = meta.source_rate;
      return {
        interval: meta.beat_interval_sec * sr,
        offset: (meta.first_beat_sec + gridOffsetRef.current) * sr,
      };
    };
    const snapBeat = (f: number) => {
      const g = grid();
      return g ? g.offset + Math.round((f - g.offset) / g.interval) * g.interval : f;
    };
    // Push the (possibly nudged) beatgrid to the engine in source frames, for the sync PLL.
    const pushBeatgrid = (gridOff: number) => {
      if (!meta || (meta.beat_interval_sec ?? 0) <= 0) return;
      const sr = meta.source_rate;
      setBeatgrid(deck, (meta.first_beat_sec + gridOff) * sr, meta.beat_interval_sec * sr).catch(swallow);
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
      togglePlay: () => {
        if (!playing) markPlayed();
        return (playing ? deckPause(deck) : deckPlay(deck)).catch(swallow);
      },
      play: () => {
        markPlayed();
        return deckPlay(deck).catch(swallow);
      },
      pause: () => deckPause(deck).catch(swallow),
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
      // Manual beatgrid anchor: shift the grid to line it up with the audio. Feeds waveform
      // rendering + beat-loop math (frontend) and the engine sync PLL (pushBeatgrid).
      nudgeGrid: (deltaSec: number) => {
        const next = gridOffsetRef.current + deltaSec;
        setGridOffset(next);
        pushBeatgrid(next);
        if (canPersist) dbSetGridOffset(path as string, next).catch(swallow);
      },
      resetGrid: () => {
        setGridOffset(0);
        pushBeatgrid(0);
        if (canPersist) dbSetGridOffset(path as string, 0).catch(swallow);
      },
      // Continuous beat-sync: follow `master` (deck index), or null to disengage.
      sync: (master: number | null) => {
        setSyncedState(master !== null);
        setDeckSync(deck, master).catch(swallow);
      },
      // Crossfader routing (0 = A, 1 = thru, 2 = B).
      setXfaderAssign: (a: number) => {
        setXfaderAssignState(a);
        setDeckXfaderAssign(deck, a).catch(swallow);
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
        if (canPersist) dbSetGain(path as string, g).catch(swallow);
      },
      setHotCue: (i: number) => {
        setHotCues((cur) => {
          const next = [...cur];
          // Set if empty, else jump to it.
          if (next[i] == null) {
            next[i] = frameRef.current;
            if (canPersist) dbSetCue(path as string, i, frameRef.current).catch(swallow);
          } else {
            const target = quantizeRef.current ? snapBeat(next[i] as number) : (next[i] as number);
            deckSeek(deck, target).catch(swallow);
          }
          return next;
        });
      },
      clearHotCue: (i: number) => {
        setHotCues((cur) => {
          const next = [...cur];
          next[i] = null;
          return next;
        });
        if (canPersist) dbClearCue(path as string, i).catch(swallow);
      },
      beatLoop: (beats: number) => {
        if (!meta || (meta.beat_interval_sec ?? 0) <= 0) return;
        const sr = meta.source_rate;
        const interval = meta.beat_interval_sec * sr;
        const offset = (meta.first_beat_sec + gridOffsetRef.current) * sr;
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
          if (canPersist) dbSetLoop(path as string, 0, inFrame, outFrame, beats).catch(swallow);
        }
      },
      loopIn: () => setLoopState((l) => ({ ...l, inFrame: frameRef.current, beats: null })),
      loopOut: () => {
        const out = frameRef.current;
        const l = loopRef.current;
        if (out > l.inFrame) {
          setLoopCmd(deck, l.inFrame, out, true).catch(swallow);
          setLoopState({ active: true, beats: null, inFrame: l.inFrame, outFrame: out });
          if (canPersist) dbSetLoop(path as string, 0, l.inFrame, out, null).catch(swallow);
        }
      },
      clearLoop: () => {
        setLoopActiveCmd(deck, false).catch(swallow);
        setLoopState((l) => ({ ...l, active: false }));
        if (canPersist) dbClearLoop(path as string, 0).catch(swallow);
      },
      toggleQuantize: () => setQuantize((q) => !q),
      // Jump the play-head by whole beats (negative = back). When quantize is on, the jump
      // starts from the nearest beat so repeated jumps stay grid-aligned.
      beatJump: (beats: number) => {
        const g = grid();
        if (!g) return;
        const base = quantizeRef.current ? snapBeat(frameRef.current) : frameRef.current;
        deckSeek(deck, Math.max(0, base + beats * g.interval)).catch(swallow);
      },
      // Momentary loop-roll with slip. Engaging loops the sub-beat region at the play-head;
      // releasing drops back in where the track would be (the engine tracks the shadow head).
      loopRoll: (beats: number, active: boolean) => {
        if (!active) {
          setLoopRollCmd(deck, 0, 0, false).catch(swallow);
          return;
        }
        const g = grid();
        if (!g) return;
        const step = g.interval * beats;
        const inFrame = Math.max(0, g.offset + Math.floor((frameRef.current - g.offset) / step) * step);
        setLoopRollCmd(deck, inFrame, inFrame + step, true).catch(swallow);
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

  const state: DeckState = { meta, frame, playing, level, tempo, keylock, gridOffset, synced, quantize, xfaderAssign, eq, filter, gain, hotCues, loop, echo, reverb, error, loading, dsp };
  return { state, actions, deck };
}

export type DeckController = ReturnType<typeof useDeck>;
