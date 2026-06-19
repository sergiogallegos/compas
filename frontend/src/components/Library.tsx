import { useState } from "react";
import { loadTrack } from "../lib/ipc";
import { useLibrary } from "../hooks/useLibrary";
import { Icon } from "./icons";

const MAGENTA = "var(--accent)";
const CYAN = "var(--stream)";

function fmtMs(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
}

/** `loadedPaths[0]` = Deck A's file path, `[1]` = Deck B's — for A/B row tags. */
export function Library({ loadedPaths }: { loadedPaths: (string | undefined)[] }) {
  const lib = useLibrary();
  const [q, setQ] = useState("");

  const filtered = lib.tracks.filter(
    (t) => q.trim() === "" || `${t.title} ${t.artist}`.toLowerCase().includes(q.toLowerCase()),
  );

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
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search title, artist…" />
          </div>
          <button className="add-btn" onClick={lib.add} disabled={lib.busy}>
            {lib.busy ? "Adding…" : "+ Add tracks"}
          </button>
          <span className="mono tl-count">{filtered.length} tracks</span>
        </div>

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
              const loadedAs =
                t.path === loadedPaths[0]
                  ? { letter: "A", color: MAGENTA }
                  : t.path === loadedPaths[1]
                    ? { letter: "B", color: CYAN }
                    : null;
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
                    <button style={{ color: MAGENTA, borderColor: `${MAGENTA}66` }} onClick={() => loadTrack(0, t.path).catch(() => {})}>A</button>
                    <button style={{ color: CYAN, borderColor: `${CYAN}66` }} onClick={() => loadTrack(1, t.path).catch(() => {})}>B</button>
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
