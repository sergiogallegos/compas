import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import {
  dbAddToCrate,
  dbCrateTracks,
  addWatchFolder,
  dbAddTag,
  dbCreateCrate,
  dbCreateSmartCrate,
  dbListCrates,
  dbRemoveTag,
  listWatchFolders,
  onLibraryChanged,
  pickFolder,
  removeWatchFolder,
  rescanWatchFolders,
  dbPlanNext,
  dbSearch,
  inTauri,
  loadTrack,
  formatKey,
  type DbCrate,
  type DbTrack,
  type KeyNotation,
} from "../lib/ipc";
import { useLibrary } from "../hooks/useLibrary";
import { Icon } from "./icons";

const MAGENTA = "var(--accent)";
// Per-deck load targets (must match App's DECK_COLORS / DECK_LETTERS).
const DECKS = [
  { letter: "A", color: "var(--accent)" },
  { letter: "B", color: "var(--stream)" },
  { letter: "C", color: "var(--status-warn)" },
  { letter: "D", color: "var(--status-ok)" },
];

function fmtMs(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
}

/** `loadedPaths[0]` = Deck A's file path, `[1]` = Deck B's — for A/B row tags. */
export const Library = forwardRef<
  HTMLElement,
  {
    loadedPaths: (string | undefined)[];
    keyNotation?: KeyNotation;
    focusTarget?: "library" | "crates" | null;
    focusSeq?: number;
  }
>(function Library({ loadedPaths, keyNotation = "camelot", focusTarget = null, focusSeq = 0 }, ref) {
  const lib = useLibrary();
  const [q, setQ] = useState("");
  const rootRef = useRef<HTMLElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [cratesPulse, setCratesPulse] = useState(false);
  useImperativeHandle(ref, () => rootRef.current as HTMLElement);
  // Search results (engine grammar) and a "suggest next" override; null = show the full library.
  const [results, setResults] = useState<DbTrack[] | null>(null);
  const [suggestFor, setSuggestFor] = useState<string | null>(null);
  const [queue, setQueue] = useState<DbTrack[]>([]);

  // Debounced search via the engine query grammar (bpm:120-128 key:8A artist:foo -live); falls
  // back to a client-side title/artist filter outside Tauri.
  useEffect(() => {
    setSuggestFor(null);
    const query = q.trim();
    if (query === "") {
      setResults(null);
      return;
    }
    if (!inTauri()) {
      setResults(
        lib.tracks.filter((t) => `${t.title} ${t.artist}`.toLowerCase().includes(query.toLowerCase())),
      );
      return;
    }
    const id = setTimeout(() => {
      dbSearch(query).then(setResults).catch(() => {});
    }, 180);
    return () => clearTimeout(id);
  }, [q, lib.tracks]);

  // Crates / playlists.
  const [crates, setCrates] = useState<DbCrate[]>([]);
  const [activeCrate, setActiveCrate] = useState<DbCrate | null>(null);
  const refreshCrates = useCallback(() => {
    if (!inTauri()) return;
    dbListCrates().then(setCrates).catch(() => {});
  }, []);
  useEffect(() => refreshCrates(), [refreshCrates]);

  const createCrate = () => {
    dbCreateCrate(`Crate ${crates.length + 1}`, false)
      .then(() => refreshCrates())
      .catch(() => {});
  };
  // Save the current search as a smart crate that re-runs the query whenever it's opened.
  const saveSearchAsCrate = () => {
    const query = q.trim();
    if (!query) return;
    dbCreateSmartCrate(`✨ ${query}`, query)
      .then(() => refreshCrates())
      .catch(() => {});
  };
  const viewCrate = (c: DbCrate) => {
    setActiveCrate(c);
    setSuggestFor(null);
    setQ("");
    dbCrateTracks(c.id).then(setResults).catch(() => {});
  };
  const addToActiveCrate = (t: DbTrack) => {
    if (!activeCrate) return;
    dbAddToCrate(activeCrate.id, t.path).then(refreshCrates).catch(() => {});
  };
  const queueTrack = (t: DbTrack) => {
    setQueue((cur) => (cur.some((q) => q.path === t.path) ? cur : [...cur, t]));
  };
  const loadNextQueued = async () => {
    const [next] = queue;
    if (!next) return;
    const emptyDeck = loadedPaths.findIndex((p) => !p);
    const targetDeck = emptyDeck >= 0 ? emptyDeck : 1;
    await loadTrack(targetDeck, next.path).catch(() => {});
    setQueue((cur) => cur.slice(1));
  };

  // Auto-mix planner: ranked next-track suggestions after `t`.
  const suggestNext = (t: DbTrack) => {
    setQ("");
    setSuggestFor(t.title);
    dbPlanNext(t.path, 12).then(setResults).catch(() => {});
  };
  const clearView = () => {
    setResults(null);
    setSuggestFor(null);
    setQ("");
  };

  // Tagging: an inline tag editor opens for one track row at a time.
  const [taggingPath, setTaggingPath] = useState<string | null>(null);
  const [tagInput, setTagInput] = useState("");
  // Re-pull whatever view is showing so a tag edit is reflected immediately.
  const reloadView = useCallback(() => {
    lib.refresh();
    if (activeCrate) dbCrateTracks(activeCrate.id).then(setResults).catch(() => {});
    else if (q.trim()) dbSearch(q.trim()).then(setResults).catch(() => {});
  }, [lib, activeCrate, q]);
  const commitTag = (path: string) => {
    const t = tagInput.trim();
    setTaggingPath(null);
    setTagInput("");
    if (t) dbAddTag(path, t).then(reloadView).catch(() => {});
  };
  const dropTag = (path: string, tag: string) => {
    dbRemoveTag(path, tag).then(reloadView).catch(() => {});
  };

  // Watched folders (auto-import).
  const [folders, setFolders] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);
  const refreshFolders = useCallback(() => {
    if (!inTauri()) return;
    listWatchFolders().then(setFolders).catch(() => {});
  }, []);
  useEffect(() => refreshFolders(), [refreshFolders]);
  // A background scan (on launch or add) imported tracks → refresh the library + folder list.
  useEffect(() => {
    if (!inTauri()) return;
    const un = onLibraryChanged(() => {
      lib.refresh();
      refreshFolders();
    });
    return () => {
      un.then((u) => u());
    };
  }, [lib, refreshFolders]);
  const addFolder = async () => {
    const dir = await pickFolder().catch(() => null);
    if (!dir) return;
    setScanning(true);
    try {
      await addWatchFolder(dir);
      refreshFolders();
      lib.refresh();
      reloadView();
    } finally {
      setScanning(false);
    }
  };
  const rescanFolders = async () => {
    setScanning(true);
    try {
      await rescanWatchFolders();
      lib.refresh();
      reloadView();
    } finally {
      setScanning(false);
    }
  };
  const dropFolder = (dir: string) => {
    removeWatchFolder(dir).then(refreshFolders).catch(() => {});
  };
  const baseName = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;

  const filtered = results ?? lib.tracks;

  useEffect(() => {
    if (!focusTarget) return;
    rootRef.current?.scrollIntoView({ block: "nearest" });
    if (focusTarget === "library") {
      searchRef.current?.focus();
      searchRef.current?.select();
      return;
    }
    setCratesPulse(true);
    const id = window.setTimeout(() => setCratesPulse(false), 850);
    return () => window.clearTimeout(id);
  }, [focusSeq, focusTarget]);

  return (
    <section className="library" ref={rootRef}>
      <aside className={`sources ${cratesPulse ? "sources--focus" : ""}`}>
        <div className="overline src-group">SOURCES</div>
        <div className="src-row src-row--active">
          <span className="src-dot" style={{ background: MAGENTA }} />
          <span className="src-name">Local Library</span>
          <span className="ctrl-tag" style={{ color: "var(--status-ok)", borderColor: "#3ddc9755" }}>
            {lib.tracks.length}
          </span>
        </div>

        <div className="overline src-group src-group--crates">
          CRATES
          <span style={{ display: "flex", gap: 4 }}>
            {q.trim() && (
              <button className="crate-add" onClick={saveSearchAsCrate} title={`Save this search as a smart crate: ${q.trim()}`}>✨</button>
            )}
            <button className="crate-add" onClick={createCrate} title="New crate">＋</button>
          </span>
        </div>
        {crates.length === 0 && <div className="src-row src-row--muted"><span className="src-name">No crates</span></div>}
        {crates.map((c) => (
          <div
            key={c.id}
            className={`src-row ${activeCrate?.id === c.id ? "src-row--active" : ""}`}
            onClick={() => viewCrate(c)}
            title={c.is_smart ? "Smart crate — re-runs its saved search when opened" : "Click to view; the ＋ on a track adds it to the selected crate"}
          >
            <span className="src-dot" style={{ background: c.is_smart ? "var(--accent)" : c.is_playlist ? "var(--stream)" : "var(--status-warn)" }} />
            <span className="src-name">{c.name}</span>
            <span className="ctrl-tag">{c.is_smart ? "✨" : c.track_count}</span>
          </div>
        ))}

        <div className="overline src-group src-group--crates">
          FOLDERS
          <span style={{ display: "flex", gap: 4 }}>
            <button className="crate-add" onClick={rescanFolders} disabled={scanning || folders.length === 0} title="Re-scan watched folders for new files">⟳</button>
            <button className="crate-add" onClick={addFolder} disabled={scanning} title="Watch a folder — auto-imports its audio files">＋</button>
          </span>
        </div>
        {folders.length === 0 && (
          <div className="src-row src-row--muted"><span className="src-name">{scanning ? "Scanning…" : "No watched folders"}</span></div>
        )}
        {folders.map((f) => (
          <div key={f} className="src-row" title={f}>
            <span className="src-dot" style={{ background: "var(--status-ok)" }} />
            <span className="src-name">{baseName(f)}</span>
            <button className="crate-add" onClick={() => dropFolder(f)} title="Stop watching this folder">✕</button>
          </div>
        ))}
      </aside>

      <div className="tracklist">
        <div className="tl-toolbar">
          <div className="search">
            <Icon name="search" size={14} />
            <input
              ref={searchRef}
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search… e.g. bpm:120-128 key:8A artist:daft -live · OR groups"
              title="Grammar: bpm:120-128 (range), key:8A, artist:/title: (fuzzy), bare word = title or artist, - to exclude. Terms AND together; 'OR' (or '|') starts a new group, e.g. artist:daft OR artist:justice"
            />
          </div>
          <button className="add-btn" onClick={lib.add} disabled={lib.busy}>
            {lib.busy ? "Adding…" : "+ Add tracks"}
          </button>
          <button className="add-btn" onClick={loadNextQueued} disabled={queue.length === 0}>
            {queue.length ? `Load next (${queue.length})` : "Queue empty"}
          </button>
          <span className="mono tl-count">{filtered.length} tracks</span>
        </div>

        {queue.length > 0 && (
          <div className="tl-banner tl-banner--queue">
            <span>
              <strong>AutoDJ queue</strong>
              {" · "}
              {queue.slice(0, 4).map((t) => t.title).join(" → ")}
              {queue.length > 4 ? ` +${queue.length - 4}` : ""}
            </span>
            <button className="chip" onClick={loadNextQueued}>load next</button>
            <button className="chip" onClick={() => setQueue([])}>clear</button>
          </div>
        )}

        {suggestFor && (
          <div className="tl-banner">
            <span>✨ Suggested next after <strong>{suggestFor}</strong> (harmonic + tempo)</span>
            <button className="chip" onClick={clearView}>clear</button>
          </div>
        )}

        <div className="tl-head tl-grid">
          <span>#</span><span>TITLE</span><span>ARTIST</span><span>BPM</span><span>KEY</span><span>TIME</span><span>LOAD</span>
        </div>

        <div className="tl-body">
          {filtered.length === 0 ? (
            <div className="tl-empty">
              Your library is empty — <strong>+ Add tracks</strong> to mount local files. Double-click
              or use the A / B buttons to load a track onto a deck.
            </div>
          ) : (
            filtered.map((t) => {
              const di = loadedPaths.findIndex((p) => p === t.path);
              const loadedAs = di >= 0 ? DECKS[di] : null;
              return (
                <div
                  key={t.path}
                  className="tl-row tl-grid"
                  style={loadedAs ? { borderLeftColor: loadedAs.color, background: `${loadedAs.color}12` } : undefined}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData("application/x-compas-track", t.path);
                    e.dataTransfer.effectAllowed = "copy";
                  }}
                  onDoubleClick={() => loadTrack(0, t.path).catch(() => {})}
                  onContextMenu={(e) => { e.preventDefault(); lib.remove(t.path); }}
                  title="Drag → deck · double-click → Deck A · right-click → remove"
                >
                  <span className="tl-tag" style={loadedAs ? { color: loadedAs.color } : undefined}>
                    {loadedAs ? loadedAs.letter : "♪"}
                  </span>
                  <span className="tl-title" style={{ display: "flex", alignItems: "center", gap: 5, minWidth: 0 }}>
                    <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{t.title}</span>
                    {t.tags.map((tg) => (
                      <button
                        key={tg}
                        onClick={(e) => { e.stopPropagation(); dropTag(t.path, tg); }}
                        title={`Remove tag — click. (filter with tag:${tg})`}
                        style={{ flex: "none", fontSize: 9, lineHeight: 1.4, padding: "1px 5px", borderRadius: 8, border: "1px solid var(--border-hairline-strong)", background: "rgba(var(--accent-rgb),0.14)", color: "var(--accent-glow)", cursor: "pointer", whiteSpace: "nowrap" }}
                      >
                        {tg} ✕
                      </button>
                    ))}
                    {taggingPath === t.path && (
                      <input
                        autoFocus
                        value={tagInput}
                        placeholder="tag…"
                        onClick={(e) => e.stopPropagation()}
                        onChange={(e) => setTagInput(e.target.value)}
                        onBlur={() => commitTag(t.path)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitTag(t.path);
                          else if (e.key === "Escape") { setTaggingPath(null); setTagInput(""); }
                        }}
                        style={{ flex: "none", width: 80, fontSize: 11, padding: "1px 5px", borderRadius: 6, border: "1px solid var(--accent)", background: "var(--surface-sunken)", color: "var(--text-primary)", outline: "none" }}
                      />
                    )}
                  </span>
                  <span className="tl-artist">{t.artist}</span>
                  <span className="mono tl-bpm">{t.bpm ? Math.round(t.bpm) : "—"}</span>
                  <span className="mono tl-key" title={t.key_name ?? undefined}>
                    {t.key_camelot ? formatKey(t.key_camelot, t.key_name, keyNotation) : "—"}
                  </span>
                  <span className="mono">{fmtMs(t.duration_ms)}</span>
                  <span className="tl-load">
                    {DECKS.map((d, i) => (
                      <button
                        key={d.letter}
                        style={{ color: d.color, borderColor: `${d.color}66` }}
                        onClick={() => loadTrack(i, t.path).catch(() => {})}
                        title={`Load onto Deck ${d.letter}`}
                      >
                        {d.letter}
                      </button>
                    ))}
                    <button
                      className="tl-next"
                      onClick={(e) => { e.stopPropagation(); suggestNext(t); }}
                      title="Suggest harmonically/tempo-compatible next tracks"
                    >
                      ✨
                    </button>
                    <button
                      className="tl-next"
                      onClick={(e) => { e.stopPropagation(); queueTrack(t); }}
                      title="Add to AutoDJ queue"
                    >
                      Q
                    </button>
                    <button
                      className="tl-next"
                      onClick={(e) => { e.stopPropagation(); setTaggingPath(t.path); setTagInput(""); }}
                      title="Add a tag"
                    >
                      🏷
                    </button>
                    {activeCrate && !activeCrate.is_smart && (
                      <button
                        className="tl-next"
                        onClick={(e) => { e.stopPropagation(); addToActiveCrate(t); }}
                        title={`Add to "${activeCrate.name}"`}
                      >
                        ＋
                      </button>
                    )}
                  </span>
                </div>
              );
            })
          )}
        </div>
      </div>
    </section>
  );
});
