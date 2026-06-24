import { useCallback, useEffect, useState } from "react";
import {
  inTauri,
  listOutputDevices,
  setCueMix,
  setCueVolume,
  setDeckCue,
  startCueOutput,
  stopCueOutput,
} from "../lib/ipc";

/**
 * Headphone / cue monitoring: pick a 2nd output device, then pre-listen (PFL) any decks on it
 * without affecting the master. The engine sums cued decks into a cue bus, blends it with the
 * master (`mix`), and plays it on the chosen device. All control state lives here; the audio
 * runs entirely in the engine.
 */
export interface CueApi {
  devices: string[];
  /** Chosen device name, or null for the system default. */
  device: string | null;
  setDevice: (name: string | null) => void;
  rescan: () => void;
  /** Whether the cue output stream is running, and the device it opened. */
  enabled: boolean;
  connectedName: string | null;
  toggle: () => Promise<void>;
  /** Cue/master blend (0 = cue only, 1 = master only) and headphone level. */
  mix: number;
  setMix: (v: number) => void;
  volume: number;
  setVolume: (v: number) => void;
  /** Which decks are currently PFL'd to the headphones. */
  cued: Set<number>;
  toggleDeckCue: (deck: number) => void;
}

export function useCue(): CueApi {
  const [devices, setDevices] = useState<string[]>([]);
  const [device, setDevice] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [connectedName, setConnectedName] = useState<string | null>(null);
  const [mix, setMixState] = useState(0); // start fully on the cue bus
  const [volume, setVolumeState] = useState(0.8);
  const [cued, setCued] = useState<Set<number>>(new Set());

  const rescan = useCallback(() => {
    if (!inTauri()) return;
    listOutputDevices().then(setDevices).catch(() => {});
  }, []);

  useEffect(() => {
    rescan();
  }, [rescan]);

  const toggle = useCallback(async () => {
    try {
      if (enabled) {
        await stopCueOutput();
        setEnabled(false);
        setConnectedName(null);
      } else {
        const name = await startCueOutput(device ?? undefined);
        setConnectedName(name);
        setEnabled(true);
        // Push current blend/level so the headphones match the UI immediately.
        setCueMix(mix).catch(() => {});
        setCueVolume(volume).catch(() => {});
      }
    } catch {
      setEnabled(false);
      setConnectedName(null);
    }
  }, [enabled, device, mix, volume]);

  const setMix = useCallback((v: number) => {
    setMixState(v);
    setCueMix(v).catch(() => {});
  }, []);

  const setVolume = useCallback((v: number) => {
    setVolumeState(v);
    setCueVolume(v).catch(() => {});
  }, []);

  const toggleDeckCue = useCallback((deck: number) => {
    setCued((cur) => {
      const next = new Set(cur);
      const active = !next.has(deck);
      if (active) next.add(deck);
      else next.delete(deck);
      setDeckCue(deck, active).catch(() => {});
      return next;
    });
  }, []);

  return {
    devices,
    device,
    setDevice,
    rescan,
    enabled,
    connectedName,
    toggle,
    mix,
    setMix,
    volume,
    setVolume,
    cued,
    toggleDeckCue,
  };
}
