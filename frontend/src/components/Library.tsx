import { useEffect, useState } from "react";
import { dbPlanNext, dbSearch, inTauri, loadTrack, type DbTrack } from "../lib/ipc";
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
export function Library({ loadedPaths }: { loadedPaths: (string | undefined)[] }) {
  const lib = useLibrary();
  const [q, setQ] = useState("");
  // Search results (engine grammar) and a "suggest next" override; null = show the full library.
  const [results, setResults] = useState<DbTrack[] | null>(null);
  const [suggestFor, setSuggestFor] = useState<string | null>(null);

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

  const filtered = results ?? lib.tracks;

  return (
    <section className="library">
      <aside className="sources">
        <div className="overline src-group">SOURCES</div>
        <div className="src-row src-row--active">
          <span className="src-dot" style={{ background: MAGENTA }} />
          <span className="src-name">Local Library</span>
          <span className="ctrl-tag" style={{ color: "var(--status-ok)", borderColor: "#3ddc9755" }}>
            {lib.tracks.length}
          </span>
        </div>
        {["Spotify", "Apple Music", "SoundCloud"].map((s) => (
          <div key={s} className="src-row src-row--muted" title="Streaming integration is paused (Phase 2)">
            <span className="src-dot" style={{ background: "var(--text-disabled)" }} />
            <span className="src-name">{s}</span>
            <span className="ctrl-tag">P2</span>
          </div>
        ))}
      </aside>

      <div className="tracklist">
        <div className="tl-toolbar">
          <div className="search">
            <Icon name="search" size={14} />
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search… e.g. bpm:120-128 key:8A artist:daft -live"
              title="Grammar: bpm:120-128 (range), key:8A, artist:/title: (fuzzy), bare word = title or artist, - to exclude"
            />
          </div>
          <button className="add-btn" onClick={lib.add} disabled={lib.busy}>
            {lib.busy ? "Adding…" : "+ Add tracks"}
          </button>
          <span className="mono tl-count">{filtered.length} tracks</span>
        </div>

        {suggestFor && (
          <div className="tl-banner">
            <span>✨ Suggested next after <strong>{suggestFor}</strong> (harmonic + tempo)</span>
            <button className="chip" onClick={clearView}>clear</button>
          </div>
        )}

        <div className="tl-head tl-grid">
          <span>#</span><span>TITLE</span><span>ARTIST</span><span>TIME</span><span>LOAD</span>
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
                  onDoubleClick={() => loadTrack(0, t.path).catch(() => {})}
                  onContextMenu={(e) => { e.preventDefault(); lib.remove(t.path); }}
                  title="Double-click → Deck A · right-click → remove"
                >
                  <span className="tl-tag" style={loadedAs ? { color: loadedAs.color } : undefined}>
                    {loadedAs ? loadedAs.letter : "♪"}
                  </span>
                  <span className="tl-title">{t.title}</span>
                  <span className="tl-artist">
                    {t.artist}
                    {(t.bpm || t.key_camelot) && (
                      <span className="tl-meta mono">
                        {t.bpm ? ` · ${Math.round(t.bpm)}` : ""}
                        {t.key_camelot ? ` · ${t.key_camelot}` : ""}
                      </span>
                    )}
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
                  </span>
                </div>
              );
            })
          )}
        </div>
      </div>
    </section>
  );
}
