import { useCallback, useEffect, useState } from "react";
import { inTauri, midiConnect, midiDisconnect, midiListPorts } from "../lib/ipc";

/**
 * Owns the single app-wide MIDI input connection. Both the synth (instrument panel) and the
 * control-mapping layer share it: raw notes/CCs arrive as the global `midi:note`/`midi:cc`
 * Tauri events regardless of who opened the port, so consumers just subscribe.
 */
export interface MidiApi {
  ports: string[];
  /** Connected port name, or null when no port is open. */
  connected: string | null;
  /** Index into `ports` selected for the next connect. */
  portIdx: number;
  setPortIdx: (i: number) => void;
  rescan: () => void;
  /** Connect the selected port, or disconnect if already connected. */
  toggle: () => Promise<void>;
}

export function useMidi(): MidiApi {
  const [ports, setPorts] = useState<string[]>([]);
  const [portIdx, setPortIdx] = useState(0);
  const [connected, setConnected] = useState<string | null>(null);

  const rescan = useCallback(() => {
    if (!inTauri()) return;
    midiListPorts()
      .then((p) => {
        setPorts(p);
        setPortIdx((i) => (i < p.length ? i : 0));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    rescan();
  }, [rescan]);

  const toggle = useCallback(async () => {
    try {
      if (connected) {
        await midiDisconnect();
        setConnected(null);
      } else if (ports.length) {
        setConnected(await midiConnect(portIdx));
      } else {
        rescan();
      }
    } catch {
      setConnected(null);
    }
  }, [connected, ports.length, portIdx, rescan]);

  return { ports, connected, portIdx, setPortIdx, rescan, toggle };
}
