import { useCallback, useState } from "react";
import { pickAudioFiles, probeTrack, type ProbedTrack } from "../lib/ipc";

const KEY = "compas_library";

function load(): ProbedTrack[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]") as ProbedTrack[];
  } catch {
    return [];
  }
}

/** Local track library, persisted in localStorage. Tracks are probed (cheap header
 *  read) on add; full BPM/key analysis happens when a track is loaded onto a deck. */
export function useLibrary() {
  const [tracks, setTracks] = useState<ProbedTrack[]>(load);
  const [busy, setBusy] = useState(false);

  const persist = useCallback((next: ProbedTrack[]) => {
    localStorage.setItem(KEY, JSON.stringify(next));
    setTracks(next);
  }, []);

  const add = useCallback(async () => {
    const paths = await pickAudioFiles();
    if (paths.length === 0) return;
    setBusy(true);
    try {
      const existing = new Set(tracks.map((t) => t.path));
      const probed: ProbedTrack[] = [];
      for (const p of paths) {
        if (existing.has(p)) continue;
        try {
          probed.push(await probeTrack(p));
        } catch {
          /* skip files that fail to probe */
        }
      }
      if (probed.length > 0) persist([...tracks, ...probed]);
    } finally {
      setBusy(false);
    }
  }, [tracks, persist]);

  const remove = useCallback(
    (path: string) => persist(tracks.filter((t) => t.path !== path)),
    [tracks, persist],
  );

  const clear = useCallback(() => persist([]), [persist]);

  return { tracks, add, remove, clear, busy };
}
