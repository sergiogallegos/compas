import { useCallback, useEffect, useState } from "react";
import {
  clearSample,
  inTauri,
  loadSample,
  pickAudioFile,
  samplerPadCount,
  setSampleLoop,
  setSamplerGain,
  stopSample,
  triggerSample,
} from "../lib/ipc";

export interface Pad {
  /** Display name when a sample is loaded, else null (empty pad). */
  name: string | null;
  /** Loop mode: triggering toggles play/stop instead of firing a one-shot. */
  loop: boolean;
  /** True while a file is decoding into this pad. */
  loading: boolean;
}

const DEFAULT_PADS = 8;
const emptyPad = (): Pad => ({ name: null, loop: false, loading: false });

/**
 * Sampler / performance pads. Load a short audio file onto a pad, then trigger it — one-shots
 * overlap (polyphonic), looped pads toggle. Playback runs in the engine on the master bus
 * (recordable); this hook only tracks pad assignments + the global level.
 */
export function useSampler() {
  const [pads, setPads] = useState<Pad[]>(() => Array.from({ length: DEFAULT_PADS }, emptyPad));
  const [gain, setGainState] = useState(0.9);

  useEffect(() => {
    if (!inTauri()) return;
    samplerPadCount()
      .then((n) => setPads((cur) => (n === cur.length ? cur : Array.from({ length: n }, emptyPad))))
      .catch(() => {});
  }, []);

  const patch = (slot: number, p: Partial<Pad>) =>
    setPads((cur) => cur.map((pad, i) => (i === slot ? { ...pad, ...p } : pad)));

  const load = useCallback(async (slot: number) => {
    const path = await pickAudioFile();
    if (!path) return;
    patch(slot, { loading: true });
    try {
      const r = await loadSample(slot, path);
      patch(slot, { name: r.name, loading: false });
    } catch {
      patch(slot, { loading: false });
    }
  }, []);

  const clear = useCallback((slot: number) => {
    clearSample(slot).catch(() => {});
    patch(slot, { name: null, loop: false });
  }, []);

  const trigger = useCallback((slot: number) => {
    triggerSample(slot).catch(() => {});
  }, []);

  const stop = useCallback((slot: number) => {
    stopSample(slot).catch(() => {});
  }, []);

  const toggleLoop = useCallback((slot: number) => {
    setPads((cur) =>
      cur.map((pad, i) => {
        if (i !== slot) return pad;
        const loop = !pad.loop;
        setSampleLoop(slot, loop).catch(() => {});
        return { ...pad, loop };
      }),
    );
  }, []);

  const setGain = useCallback((g: number) => {
    setGainState(g);
    setSamplerGain(g).catch(() => {});
  }, []);

  return { pads, gain, load, clear, trigger, stop, toggleLoop, setGain };
}

export type SamplerController = ReturnType<typeof useSampler>;
