import { useCallback, useState } from "react";
import * as sp from "../lib/spotify";

export function useSpotify() {
  const [connected, setConnected] = useState(sp.isConnected());
  const [clientId, setClientId] = useState(sp.getClientId());
  const [results, setResults] = useState<sp.SpotifyTrack[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const saveClientId = useCallback((id: string) => {
    sp.setClientId(id);
    setClientId(id);
  }, []);

  const connect = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      await sp.connect();
      setConnected(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const disconnect = useCallback(() => {
    sp.disconnect();
    setConnected(false);
    setResults([]);
  }, []);

  const search = useCallback(async (q: string) => {
    setError(null);
    setBusy(true);
    try {
      setResults(await sp.search(q));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return { connected, clientId, saveClientId, connect, disconnect, results, search, busy, error };
}
