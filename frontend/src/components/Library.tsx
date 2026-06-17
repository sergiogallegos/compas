import { useState } from "react";
import type { DeckLoaded } from "../lib/ipc";
import { Icon } from "./icons";

export interface LibRow {
  letter: string;
  color: string;
  meta: DeckLoaded;
}

function fmtMs(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
}

const SOURCES = [
  { name: "Local Library", active: true, tag: null },
  { name: "Spotify", active: false, tag: "CTRL" },
  { name: "Apple Music", active: false, tag: "CTRL" },
  { name: "SoundCloud", active: false, tag: "CTRL" },
];

export function Library({ rows }: { rows: LibRow[] }) {
  const [q, setQ] = useState("");
  const filtered = rows.filter(
    (r) =>
      q.trim() === "" ||
      `${r.meta.title} ${r.meta.artist}`.toLowerCase().includes(q.toLowerCase()),
  );

  return (
    <section className="library">
      <aside className="sources">
        <div className="overline src-group">SOURCES</div>
        {SOURCES.map((s) => (
          <div key={s.name} className={`src-row ${s.active ? "src-row--active" : ""} ${s.tag ? "src-row--ctrl" : ""}`}>
            <span className="src-dot" style={{ background: s.active ? "var(--accent)" : "var(--text-tertiary)" }} />
            <span className="src-name">{s.name}</span>
            {s.tag && <span className="ctrl-tag">{s.tag}</span>}
          </div>
        ))}
        <div className="overline src-group">CRATES</div>
        <div className="src-row src-row--muted"><Icon name="folder" size={14} /> <span className="src-name">Coming with the catalog system (P2)</span></div>
      </aside>

      <div className="tracklist">
        <div className="tl-toolbar">
          <div className="search">
            <Icon name="search" size={14} />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search title, artist…" />
          </div>
          <span className="mono tl-count">{filtered.length} loaded</span>
        </div>

        <div className="tl-head">
          <span>#</span><span>TITLE</span><span>ARTIST</span><span>BPM</span><span>KEY</span><span>TIME</span><span>SOURCE</span>
        </div>

        <div className="tl-body">
          {filtered.length === 0 ? (
            <div className="tl-empty">
              No catalog yet — use <strong>Load…</strong> on a deck to mount a track. A browsable
              local library + streaming sources arrive in Phase 2.
            </div>
          ) : (
            filtered.map((r) => (
              <div key={r.letter} className="tl-row" style={{ borderLeftColor: r.color, background: `${r.color}12` }}>
                <span className="tl-tag" style={{ color: r.color }}>{r.letter}</span>
                <span className="tl-title">{r.meta.title}</span>
                <span className="tl-artist">{r.meta.artist}</span>
                <span className="mono">{r.meta.bpm > 0 ? r.meta.bpm.toFixed(1) : "—"}</span>
                <span className="mono">{r.meta.key_camelot || "—"}</span>
                <span className="mono">{fmtMs(r.meta.duration_ms)}</span>
                <span className="tl-src">LOCAL</span>
              </div>
            ))
          )}
        </div>
      </div>
    </section>
  );
}
