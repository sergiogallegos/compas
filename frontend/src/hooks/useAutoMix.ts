import { useCallback, useEffect, useRef, useState } from "react";
import type { DeckController } from "./useDeck";

/** Crossfade length, in beats of the outgoing track. */
const TRANSITION_BEATS = 16;
/** Auto-trigger this many beats (plus the crossfade) before the live track ends. */
const LEAD_BEATS = 16;
/** EQ low "kill" level used for the bass swap. */
const BASS_KILL_DB = -26;
/** Transition animation tick (20 Hz; the engine smooths crossfader + EQ between steps). */
const TICK_MS = 50;

/**
 * Auto-mix / transition orchestration. Frontend-only: it drives the existing sync,
 * crossfader, and EQ controls to beatmatch the incoming deck and crossfade into it with a
 * bass swap. AUTO mode auto-triggers near the end of the live track; MIX NOW triggers it
 * immediately. No audio-thread changes — the transition is a timed sequence of control calls.
 */
export function useAutoMix(
  decks: [DeckController, DeckController],
  applyCrossfade: (v: number) => void,
) {
  const [enabled, setEnabled] = useState(false);
  const [transitioning, setTransitioning] = useState(false);
  // Latest decks/crossfade/flag for the timers (which capture once).
  const ref = useRef({ decks, applyCrossfade, transitioning });
  ref.current = { decks, applyCrossfade, transitioning };
  const timer = useRef<number | null>(null);

  const startTransition = useCallback((from: number, to: number) => {
    const { decks, applyCrossfade } = ref.current;
    const fromCtrl = decks[from];
    const toCtrl = decks[to];
    const fromMeta = fromCtrl.state.meta;
    const toMeta = toCtrl.state.meta;
    if (!fromMeta || !toMeta || !fromCtrl.state.dsp || !toCtrl.state.dsp) return;

    setTransitioning(true);
    // Cue the incoming deck at its first downbeat, beat-sync it to the live deck, start it.
    const startFrac =
      toMeta.frames > 0
        ? Math.min(0.99, ((toMeta.first_beat_sec || 0) * toMeta.source_rate) / toMeta.frames)
        : 0;
    toCtrl.actions.seekFrac(startFrac);
    toCtrl.actions.sync(from);
    toCtrl.actions.play();

    const fromEq = { ...fromCtrl.state.eq };
    const toEq = { ...toCtrl.state.eq };
    const effBpm = Math.max(60, (fromMeta.bpm || 120) * fromCtrl.state.tempo);
    const durMs = ((TRANSITION_BEATS * 60) / effBpm) * 1000;
    const startXf = to === 1 ? 0 : 1;
    const endXf = to === 1 ? 1 : 0;
    const t0 = performance.now();

    if (timer.current) clearInterval(timer.current);
    timer.current = window.setInterval(() => {
      const p = Math.min(1, (performance.now() - t0) / durMs);
      applyCrossfade(startXf + (endXf - startXf) * p);
      // Bass swap: fade the outgoing low to kill, bring the incoming low up from kill.
      fromCtrl.actions.setEq({ ...fromEq, low: fromEq.low + (BASS_KILL_DB - fromEq.low) * p });
      toCtrl.actions.setEq({ ...toEq, low: BASS_KILL_DB + (toEq.low - BASS_KILL_DB) * p });
      if (p >= 1) {
        if (timer.current) {
          clearInterval(timer.current);
          timer.current = null;
        }
        applyCrossfade(endXf);
        fromCtrl.actions.pause();
        fromCtrl.actions.setEq(fromEq); // restore the (now-silent) outgoing EQ for next time
        fromCtrl.actions.sync(null);
        toCtrl.actions.sync(null); // incoming is now the free-running live deck
        setTransitioning(false);
      }
    }, TICK_MS);
  }, []);

  // Pick the live deck (the single one playing) and the loaded, idle target.
  const pickLiveTarget = (decks: [DeckController, DeckController]): [number, number] | null => {
    const playing = [decks[0].state.playing, decks[1].state.playing];
    if (!playing[0] && !playing[1]) return null;
    const live = playing[0] && !playing[1] ? 0 : playing[1] && !playing[0] ? 1 : 0;
    const target = live === 0 ? 1 : 0;
    if (!decks[target].state.meta || decks[target].state.playing) return null;
    return [live, target];
  };

  // AUTO mode: poll for the live track nearing its end, then transition.
  useEffect(() => {
    if (!enabled) return;
    const id = window.setInterval(() => {
      const { decks, transitioning } = ref.current;
      if (transitioning) return;
      const lt = pickLiveTarget(decks);
      if (!lt) return;
      const [live, target] = lt;
      const lm = decks[live].state.meta!;
      const effBpm = Math.max(60, (lm.bpm || 120) * decks[live].state.tempo);
      const remainingSec =
        (lm.frames - decks[live].state.frame) / (lm.source_rate * decks[live].state.tempo);
      const leadSec = ((LEAD_BEATS + TRANSITION_BEATS) * 60) / effBpm;
      if (remainingSec <= leadSec) startTransition(live, target);
    }, 500);
    return () => clearInterval(id);
  }, [enabled, startTransition]);

  useEffect(
    () => () => {
      if (timer.current) clearInterval(timer.current);
    },
    [],
  );

  const mixNow = useCallback(() => {
    const { decks, transitioning } = ref.current;
    if (transitioning) return;
    const lt = pickLiveTarget(decks);
    if (lt) startTransition(lt[0], lt[1]);
  }, [startTransition]);

  const toggle = useCallback(() => setEnabled((e) => !e), []);

  return { enabled, transitioning, toggle, mixNow };
}
