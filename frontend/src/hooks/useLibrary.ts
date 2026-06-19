import { useCallback, useEffect, useState } from "react";
import {
  dbAddTrack,
  dbListTracks,
  dbRemoveTrack,
  inTauri,
  pickAudioFiles,
  type DbTrack,
} from "../lib/ipc";

const LEGACY_KEY = "compas_library";

/** One-time import of the old localStorage library into the SQLite DB, then drop the key. */
async function migrateLegacy(): Promise<void> {
  let paths: string[] = [];
  try {
    const raw = localStorage.getItem(LEGACY_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw) as { path?: string }[];
    paths = parsed.map((t) => t.path).filter((p): p is string => typeof p === "string");
  } catch {
    localStorage.removeItem(LEGACY_KEY);
    return;
  }
  for (const p of paths) {
    try {
      await dbAddTrack(p);
    } catch {
      /* skip files that no longer probe */
    }
  }
  localStorage.removeItem(LEGACY_KEY);
}

/** Local track library, persisted in SQLite. Tracks are probed (cheap header read) on add;
 *  full BPM/key analysis is cached when a track is loaded onto a deck. */
export function useLibrary() {
  const [tracks, setTracks] = useState<DbTrack[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    if (!inTauri()) return;
    dbListTracks().then(setTracks).catch(() => {});
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    migrateLegacy().finally(refresh);
  }, [refresh]);

  const add = useCallback(async () => {
    const paths = await pickAudioFiles();
    if (paths.length === 0) return;
    setBusy(true);
    try {
      for (const p of paths) {
        try {
          await dbAddTrack(p);
        } catch {
          /* skip files that fail to probe */
        }
      }
      refresh();
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const remove = useCallback(
    (path: string) => {
      dbRemoveTrack(path)
        .then(refresh)
        .catch(() => {});
    },
    [refresh],
  );

  return { tracks, add, remove, busy, refresh };
}
