import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  setDeckSyncLive,
  setDeckSyncInternal,
  setDeckXfaderAssign,
  setDeckEcho,
  setDeckReverb,
  setDeckFlanger,
  setDeckCrusher,
  setDeckEq,
  setDeckFilter,
  setDeckGain,
  setDeckTempo,
  setLoop as setLoopCmd,
  setLoopActive as setLoopActiveCmd,
  setLoopRoll as setLoopRollCmd,
  scaleLoop as scaleLoopCmd,
  moveLoop as moveLoopCmd,
  cueButton as cueButtonCmd,
  setCueMode as setCueModeCmd,
  setDeckSyncMode as setDeckSyncModeCmd,
  setSyncLeader as setSyncLeaderCmd,
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
  /** Manual loop-in set and waiting for OUT (no active loop yet) — for IN/OUT button feedback. */
  armed?: boolean;
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

export interface FlangerState {
  active: boolean;
  /** LFO sweep period in beats (beat-synced). */
  beats: number;
  /** Sweep width 0..1 (also drives feedback/resonance). */
  depth: number;
}

export interface CrusherState {
  active: boolean;
  /** Crush amount 0..1 (0 = clean ~16-bit, 1 = ~2-bit). */
  crush: number;
  /** Sample-rate reduction 0..1 (0 = off, 1 = heavy decimation). */
  down: number;
}

/** Fallback echo time (seconds) for one beat when a track has no beatgrid. */
const NO_GRID_BEAT_SEC = 0.5;

export interface DeckState {
  meta: DeckLoaded | null;
  frame: number;
  playing: boolean;
  level: number;
  /** Effective advance in source frames/sec (for play-head extrapolation). */
  rate: number;
  /** Measured output (DAC) latency in seconds. */
  latencySecs: number;
  /** `performance.now()` when `frame` was last sampled. */
  frameAt: number;
  tempo: number;
  /** Key-lock (master tempo): tempo changes preserve pitch. */
  keylock: boolean;
  /** Manual beatgrid nudge (seconds) added to the analyzed first-beat anchor. */
  gridOffset: number;
  /** True while this deck is a continuous sync follower. */
  synced: boolean;
  /** Following the live mic/aux beat clock (tempo-match). Mutually exclusive with deck sync. */
  syncLive: boolean;
  /** Following the internal master clock. Mutually exclusive with deck sync and live sync. */
  syncInternal: boolean;
  /** Quantize: snap hot-cue jumps and beat-jumps to the beatgrid. */
  quantize: boolean;
  /** Main-cue behavior: 0 = CDJ, 1 = gated. */
  cueMode: number;
  /** Sync mode: 0 = full tempo+phase, 1 = tempo-only. */
  syncMode: number;
  /** Whether this deck is pinned as the explicit sync leader. */
  isLeader: boolean;
  /** Crossfader routing: 0 = A side, 1 = thru, 2 = B side. */
  xfaderAssign: number;
  eq: Eq;
  filter: number;
  gain: number;
  hotCues: (number | null)[];
  loop: LoopState;
  echo: EchoState;
  reverb: ReverbState;
  flanger: FlangerState;
  crusher: CrusherState;
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

export function useDeck(deck: number, dsp = true, clock?: { active: boolean; bpm: number }) {
  const [meta, setMeta] = useState<DeckLoaded | null>(null);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [level, setLevel] = useState(0);
  // Play-head extrapolation inputs: advance rate (source frames/sec), DAC latency, and the
  // wall-clock time `frame` was last sampled — the UI smooths between 30 Hz position events.
  const [rate, setRate] = useState(0);
  const [latencySecs, setLatencySecs] = useState(0);
  const [frameAt, setFrameAt] = useState(0);
  const [tempo, setTempo] = useState(1);
  const [keylock, setKeylockState] = useState(false);
  const [gridOffset, setGridOffset] = useState(0);
  const [synced, setSyncedState] = useState(false);
  const [syncLive, setSyncLiveState] = useState(false);
  const [syncInternal, setSyncInternalState] = useState(false);
  const [quantize, setQuantize] = useState(false);
  // Main-cue behavior (0 = CDJ, 1 = gated) and sync mode (0 = full, 1 = tempo-only) + leader pin.
  const [cueMode, setCueModeState] = useState(0);
  const [syncMode, setSyncModeState] = useState(0);
  const [isLeader, setIsLeaderState] = useState(false);
  // Default routing: deck 0 → A, deck 1 → B, decks C/D → through.
  const [xfaderAssign, setXfaderAssignState] = useState(deck === 0 ? 0 : deck === 1 ? 2 : 1);
  const [eq, setEqState] = useState<Eq>({ hi: 0, mid: 0, low: 0 });
  const [filter, setFilterState] = useState(0);
  const [gain, setGainState] = useState(1);
  const [hotCues, setHotCues] = useState<(number | null)[]>(Array(8).fill(null));
  const [loop, setLoopState] = useState<LoopState>({ active: false, beats: null, inFrame: 0, outFrame: 0 });
  const [echo, setEchoState] = useState<EchoState>({ active: false, beats: 0.5, depth: 0.5 });
  const [reverb, setReverbState] = useState<ReverbState>({ active: false, size: 0.6, mix: 0.3 });
  const [flanger, setFlangerState] = useState<FlangerState>({ active: false, beats: 4, depth: 0.6 });
  const [crusher, setCrusherState] = useState<CrusherState>({ active: false, crush: 0.5, down: 0.3 });
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
  const flangerRef = useRef(flanger);
  flangerRef.current = flanger;
  const crusherRef = useRef(crusher);
  crusherRef.current = crusher;

  // Beat-synced FX derive their beat length from the deck's analyzed grid — UNLESS the deck follows
  // the internal master clock, in which case the engine rate-matches the deck to the clock, so the
  // deck's audio beats at the clock tempo and the FX must track that instead.
  const fxBeatSec =
    syncInternal && clock?.active && clock.bpm > 0
      ? 60 / clock.bpm
      : (meta?.beat_interval_sec ?? 0) > 0
        ? meta!.beat_interval_sec
        : NO_GRID_BEAT_SEC;

  // Translate the UI echo (beat-synced time + single "depth") to engine params.
  const applyEcho = useCallback(
    (e: EchoState) => {
      const timeSec = fxBeatSec * e.beats;
      const feedback = 0.15 + e.depth * 0.7; // 0.15..0.85
      const mix = e.active ? e.depth * 0.5 : 0; // up to half-wet
      setDeckEcho(deck, e.active, timeSec, feedback, mix).catch(() => {});
    },
    [deck, fxBeatSec],
  );
  // Beat-synced flanger: the LFO period is `beats` long; depth drives sweep + resonance.
  const applyFlanger = useCallback(
    (f: FlangerState) => {
      const period = fxBeatSec * f.beats;
      const rateHz = period > 0 ? 1 / period : 0.3;
      const feedback = 0.35 + f.depth * 0.5; // 0.35..0.85
      const mix = f.active ? 0.5 : 0; // classic 50/50 flange
      setDeckFlanger(deck, f.active, rateHz, f.depth, feedback, mix).catch(() => {});
    },
    [deck, fxBeatSec],
  );
  // Re-push active beat-synced FX when the effective tempo source changes (clock BPM edit, INT
  // toggle, or a new grid on load) so the audible delay/LFO keeps tracking without a chip re-toggle.
  useEffect(() => {
    if (!dsp) return;
    if (echoRef.current.active) applyEcho(echoRef.current);
    if (flangerRef.current.active) applyFlanger(flangerRef.current);
  }, [dsp, applyEcho, applyFlanger]);
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
        setFlangerState((f) => ({ ...f, active: false }));
        setCrusherState((c) => ({ ...c, active: false }));
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
        setRate(e.rate ?? 0);
        setLatencySecs(e.latency_secs ?? 0);
        setFrameAt(performance.now());
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
    // Beat-synced echo/flanger pushes are defined at component scope (they track the internal
    // clock via `fxBeatSec` and re-push on tempo change); alias them here for the setters below.
    const pushEcho = applyEcho;
    const pushFlanger = applyFlanger;
    const pushReverb = (r: ReverbState) => {
      setDeckReverb(deck, r.active, r.size, r.active ? r.mix : 0).catch(swallow);
    };
    // Bitcrusher: crush 0..1 → 16..2 bits; down 0..1 → 1..32× sample-and-hold. Full wet.
    const pushCrusher = (c: CrusherState) => {
      const bits = Math.round(16 - c.crush * 14);
      const downsample = 1 + Math.round(c.down * 31);
      setDeckCrusher(deck, c.active, bits, downsample, c.active ? 1 : 0).catch(swallow);
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
        if (master !== null) {
          // The engine cancels live and internal-clock sync; mirror it in the UI.
          setSyncLiveState(false);
          setSyncInternalState(false);
        }
        setDeckSync(deck, master).catch(swallow);
      },
      // Tempo-match the live mic/aux beat clock (mutually exclusive with deck and internal sync).
      toggleSyncLive: () => {
        const next = !syncLive;
        setSyncLiveState(next);
        if (next) {
          setSyncedState(false);
          setSyncInternalState(false);
        }
        setDeckSyncLive(deck, next).catch(swallow);
      },
      // Tempo/phase-match the internal master clock (mutually exclusive with deck and live sync).
      toggleSyncInternal: () => {
        const next = !syncInternal;
        setSyncInternalState(next);
        if (next) {
          setSyncedState(false);
          setSyncLiveState(false);
        }
        setDeckSyncInternal(deck, next).catch(swallow);
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
      // Arm the loop: store the in-point and light IN until OUT lands (no engine loop yet).
      loopIn: () => setLoopState((l) => ({ ...l, inFrame: frameRef.current, beats: null, armed: true })),
      loopOut: () => {
        const out = frameRef.current;
        const l = loopRef.current;
        if (out > l.inFrame) {
          setLoopCmd(deck, l.inFrame, out, true).catch(swallow);
          setLoopState({ active: true, beats: null, inFrame: l.inFrame, outFrame: out, armed: false });
          if (canPersist) dbSetLoop(path as string, 0, l.inFrame, out, null).catch(swallow);
        }
      },
      clearLoop: () => {
        setLoopActiveCmd(deck, false).catch(swallow);
        setLoopState((l) => ({ ...l, active: false, armed: false }));
        if (canPersist) dbClearLoop(path as string, 0).catch(swallow);
      },
      // Halve/double the active loop, anchored at the loop-in. Mirror the new length locally.
      scaleLoop: (factor: number) => {
        scaleLoopCmd(deck, factor).catch(swallow);
        setLoopState((l) => {
          if (!l.active || l.outFrame <= l.inFrame) return l;
          const len = Math.max(8, (l.outFrame - l.inFrame) * factor);
          return {
            ...l,
            outFrame: l.inFrame + len,
            beats: l.beats != null ? l.beats * factor : null,
          };
        });
      },
      // Shift the active loop by whole beats (or ~50 ms when there's no grid).
      moveLoop: (deltaBeats: number) => {
        const g = grid();
        const delta = g ? g.interval * deltaBeats : (meta?.source_rate ?? 44100) * 0.05 * Math.sign(deltaBeats);
        moveLoopCmd(deck, delta).catch(swallow);
        setLoopState((l) =>
          l.active ? { ...l, inFrame: Math.max(0, l.inFrame + delta), outFrame: l.outFrame + delta } : l,
        );
      },
      // Main CUE button (press/release) → engine cue state machine (CDJ preview / gated stutter).
      cueButton: (pressed: boolean) => cueButtonCmd(deck, pressed).catch(swallow),
      setCueMode: (mode: number) => {
        setCueModeState(mode);
        setCueModeCmd(deck, mode === 1 ? 1 : 0).catch(swallow);
      },
      // Sync mode: full tempo+phase vs tempo-only.
      setSyncMode: (mode: number) => {
        setSyncModeState(mode);
        setDeckSyncModeCmd(deck, mode === 1 ? 1 : 0).catch(swallow);
      },
      // Pin/unpin this deck as the explicit sync leader.
      toggleLeader: () => {
        const next = !isLeader;
        setIsLeaderState(next);
        setSyncLeaderCmd(deck, next).catch(swallow);
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
      toggleFlanger: () => {
        const next = { ...flangerRef.current, active: !flangerRef.current.active };
        setFlangerState(next);
        pushFlanger(next);
      },
      setFlangerBeats: (beats: number) => {
        const next = { ...flangerRef.current, beats };
        setFlangerState(next);
        if (next.active) pushFlanger(next);
      },
      setFlangerDepth: (depth: number) => {
        const next = { ...flangerRef.current, depth };
        setFlangerState(next);
        if (next.active) pushFlanger(next);
      },
      toggleCrusher: () => {
        const next = { ...crusherRef.current, active: !crusherRef.current.active };
        setCrusherState(next);
        pushCrusher(next);
      },
      setCrusherCrush: (crush: number) => {
        const next = { ...crusherRef.current, crush };
        setCrusherState(next);
        if (next.active) pushCrusher(next);
      },
      setCrusherDown: (down: number) => {
        const next = { ...crusherRef.current, down };
        setCrusherState(next);
        if (next.active) pushCrusher(next);
      },
    };
  }, [deck, playing, tempo, meta, isLeader, syncLive, syncInternal, applyEcho, applyFlanger]);

  const state: DeckState = { meta, frame, playing, level, rate, latencySecs, frameAt, tempo, keylock, gridOffset, synced, syncLive, syncInternal, quantize, cueMode, syncMode, isLeader, xfaderAssign, eq, filter, gain, hotCues, loop, echo, reverb, flanger, crusher, error, loading, dsp };
  return { state, actions, deck };
}

export type DeckController = ReturnType<typeof useDeck>;
