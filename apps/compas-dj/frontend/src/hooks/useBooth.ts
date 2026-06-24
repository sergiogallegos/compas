import { useCallback, useEffect, useState } from "react";
import {
  inTauri,
  listOutputDevices,
  setBoothVolume,
  startBoothOutput,
  stopBoothOutput,
} from "../lib/ipc";

/** Booth / monitor output: post-master mix on an optional third output device. */
export interface BoothApi {
  devices: string[];
  device: string | null;
  setDevice: (name: string | null) => void;
  rescan: () => void;
  enabled: boolean;
  connectedName: string | null;
  toggle: () => Promise<void>;
  volume: number;
  setVolume: (v: number) => void;
}

export function useBooth(): BoothApi {
  const [devices, setDevices] = useState<string[]>([]);
  const [device, setDevice] = useState<string | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [connectedName, setConnectedName] = useState<string | null>(null);
  const [volume, setVolumeState] = useState(0.8);

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
        await stopBoothOutput();
        setEnabled(false);
        setConnectedName(null);
      } else {
        const name = await startBoothOutput(device ?? undefined);
        setConnectedName(name);
        setEnabled(true);
        setBoothVolume(volume).catch(() => {});
      }
    } catch {
      setEnabled(false);
      setConnectedName(null);
    }
  }, [enabled, device, volume]);

  const setVolume = useCallback((v: number) => {
    setVolumeState(v);
    setBoothVolume(v).catch(() => {});
  }, []);

  return {
    devices,
    device,
    setDevice,
    rescan,
    enabled,
    connectedName,
    toggle,
    volume,
    setVolume,
  };
}
