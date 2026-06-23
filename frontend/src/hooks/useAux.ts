import { useCallback, useEffect, useState } from "react";
import {
  inTauri,
  listInputDevices,
  liveBeatClock,
  setAuxGain,
  startAuxInput,
  stopAuxInput,
  type LiveBeat,
} from "../lib/ipc";

/** Aux / microphone input: capture from an input device and sum it into the master bus. */
export interface AuxApi {
  devices: string[];
  device: string | null;
  setDevice: (name: string | null) => void;
  rescan: () => void;
  enabled: boolean;
  connectedName: string | null;
  toggle: () => Promise<void>;
  gain: number;
  setGain: (v: number) => void;
  /** Live beat-tracker readout (tempo/phase/confidence/lock), or null when capture is off. */
  live: LiveBeat | null;
}

export function useAux(): AuxApi {
  const [devices, setDevices] = useState<string[]>([]);
  const [device, setDevice] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [connectedName, setConnectedName] = useState<string | null>(null);
  const [gain, setGainState] = useState(1.0);
  const [live, setLive] = useState<LiveBeat | null>(null);

  const rescan = useCallback(() => {
    if (!inTauri()) return;
    listInputDevices().then(setDevices).catch(() => {});
  }, []);

  useEffect(() => {
    rescan();
  }, [rescan]);

  // Poll the live beat clock while capture is on (≈8 Hz — snappy enough for a BPM readout).
  useEffect(() => {
    if (!enabled || !inTauri()) {
      setLive(null);
      return;
    }
    let alive = true;
    const id = window.setInterval(() => {
      liveBeatClock()
        .then((b) => {
          if (alive) setLive(b);
        })
        .catch(() => {});
    }, 125);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [enabled]);

  const toggle = useCallback(async () => {
    try {
      if (enabled) {
        await stopAuxInput();
        setEnabled(false);
        setConnectedName(null);
      } else {
        const name = await startAuxInput(device ?? undefined);
        setConnectedName(name);
        setEnabled(true);
        setAuxGain(gain).catch(() => {});
      }
    } catch {
      setEnabled(false);
      setConnectedName(null);
    }
  }, [enabled, device, gain]);

  const setGain = useCallback((v: number) => {
    setGainState(v);
    setAuxGain(v).catch(() => {});
  }, []);

  return {
    devices,
    device,
    setDevice,
    rescan,
    enabled,
    connectedName,
    toggle,
    gain,
    setGain,
    live,
  };
}
