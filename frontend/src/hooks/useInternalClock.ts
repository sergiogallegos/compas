import { useCallback, useState } from "react";
import { setInternalClock } from "../lib/ipc";

/** Internal master clock: a free-running metronome that can act as a virtual sync leader so decks
 *  (via each deck's INT chip) lock to a global tempo with nothing playing. */
export interface InternalClockApi {
  active: boolean;
  bpm: number;
  toggle: () => void;
  setBpm: (bpm: number) => void;
}

const MIN_BPM = 40;
const MAX_BPM = 220;

export function useInternalClock(): InternalClockApi {
  const [active, setActive] = useState(false);
  const [bpm, setBpmState] = useState(120);

  const toggle = useCallback(() => {
    setActive((on) => {
      const next = !on;
      setInternalClock(next, bpm).catch(() => {});
      return next;
    });
  }, [bpm]);

  const setBpm = useCallback(
    (next: number) => {
      const clamped = Math.min(MAX_BPM, Math.max(MIN_BPM, next));
      setBpmState(clamped);
      // Push the tempo regardless of `active` so it's ready when the clock is enabled; the engine
      // only drives synced decks while the clock is active.
      setInternalClock(active, clamped).catch(() => {});
    },
    [active],
  );

  return { active, bpm, toggle, setBpm };
}
