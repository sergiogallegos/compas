import { useCallback, useEffect, useState } from "react";
import {
  inTauri,
  listInputDevices,
  setAuxGain,
  startAuxInput,
  stopAuxInput,
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
}

export function useAux(): AuxApi {
  const [devices, setDevices] = useState<string[]>([]);
  const [device, setDevice] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [connectedName, setConnectedName] = useState<string | null>(null);
  const [gain, setGainState] = useState(1.0);

  const rescan = useCallback(() => {
    if (!inTauri()) return;
    listInputDevices().then(setDevices).catch(() => {});
  }, []);

  useEffect(() => {
    rescan();
  }, [rescan]);

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
  };
}
