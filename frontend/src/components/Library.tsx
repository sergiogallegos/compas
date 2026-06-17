import { useState, type KeyboardEvent } from "react";
import type { DeckLoaded } from "../lib/ipc";
import { useSpotify } from "../hooks/useSpotify";
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

type Source = "local" | "spotify";

export function Library({ rows }: { rows: LibRow[] }) {
  const [source, setSource] = useState<Source>("local");
  const [q, setQ] = useState("");
  const spotify = useSpotify();
  const [clientIdInput, setClientIdInput] = useState(spotify.clientId);

  const localFiltered = rows.filter(
    (r) => q.trim() === "" || `${r.meta.title} ${r.meta.artist}`.toLowerCase().includes(q.toLowerCase()),
  );

  const onSearchKey = (e: KeyboardEvent) => {
    if (source === "spotify" && e.key === "Enter") spotify.search(q);
  };

  return (
    <section className="library">
      <aside className="sources">
        <div className="overline src-group">SOURCES</div>
        <button className={`src-row ${source === "local" ? "src-row--active" : ""}`} onClick={() => setSource("local")}>
          <span className="src-dot" style={{ background: source === "local" ? "var(--accent)" : "var(--text-tertiary)" }} />
          <span className="src-name">Local Library</span>
        </button>
        <button className={`src-row src-row--ctrl ${source === "spotify" ? "src-row--active" : ""}`} onClick={() => setSource("spotify")}>
          <span className="src-dot" style={{ background: "#1db954" }} />
          <span className="src-name">Spotify</span>
          <span className="ctrl-tag">{spotify.connected ? "ON" : "CTRL"}</span>
        </button>
        {["Apple Music", "SoundCloud"].map((s) => (
          <div key={s} className="src-row src-row--muted">
            <span className="src-dot" style={{ background: "var(--text-tertiary)" }} />
            <span className="src-name">{s}</span>
            <span className="ctrl-tag">P2+</span>
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
              onKeyDown={onSearchKey}
              placeholder={source === "spotify" ? "Search Spotify — press Enter" : "Search title, artist…"}
            />
          </div>
          <span className="mono tl-count">
            {source === "spotify" ? `${spotify.results.length} results` : `${localFiltered.length} loaded`}
          </span>
        </div>

        {source === "spotify" && !spotify.connected ? (
          <SpotifyConnect
            clientIdInput={clientIdInput}
            setClientIdInput={setClientIdInput}
            onSave={() => spotify.saveClientId(clientIdInput)}
            onConnect={spotify.connect}
            busy={spotify.busy}
            error={spotify.error}
          />
        ) : (
          <>
            <div className="tl-head">
              <span>#</span><span>TITLE</span><span>ARTIST</span><span>BPM</span><span>KEY</span><span>TIME</span><span>SOURCE</span>
            </div>
            <div className="tl-body">
              {source === "local"
                ? localFiltered.length === 0
                  ? <div className="tl-empty">No catalog yet — use <strong>Load…</strong> on a deck to mount a track.</div>
                  : localFiltered.map((r) => (
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
                : spotify.results.length === 0
                  ? <div className="tl-empty">{spotify.busy ? "Searching…" : "Search Spotify above (press Enter). Playback lands in 2b."}</div>
                  : spotify.results.map((t) => (
                      <div key={t.id} className="tl-row">
                        <span className="tl-tag" style={{ color: "var(--stream)" }}>♫</span>
                        <span className="tl-title">{t.title}</span>
                        <span className="tl-artist">{t.artist}</span>
                        <span className="mono">—</span>
                        <span className="mono">—</span>
                        <span className="mono">{fmtMs(t.durationMs)}</span>
                        <span className="tl-src" style={{ color: "var(--stream)" }}>SPOTIFY 🔒</span>
                      </div>
                    ))}
            </div>
          </>
        )}
      </div>
    </section>
  );
}

function SpotifyConnect({
  clientIdInput,
  setClientIdInput,
  onSave,
  onConnect,
  busy,
  error,
}: {
  clientIdInput: string;
  setClientIdInput: (v: string) => void;
  onSave: () => void;
  onConnect: () => void;
  busy: boolean;
  error: string | null;
}) {
  return (
    <div className="sp-connect">
      <h3>Connect Spotify</h3>
      <ol className="sp-steps">
        <li>Create an app at <span className="mono">developer.spotify.com/dashboard</span></li>
        <li>Add redirect URI <span className="mono">http://127.0.0.1:14565/callback</span></li>
        <li>Paste the app's <strong>Client ID</strong> below (needs Spotify Premium to play)</li>
      </ol>
      <div className="sp-row">
        <input
          className="sp-input"
          placeholder="Spotify Client ID"
          value={clientIdInput}
          onChange={(e) => setClientIdInput(e.target.value)}
          onBlur={onSave}
        />
        <button
          className="sp-btn"
          onClick={() => {
            onSave();
            onConnect();
          }}
          disabled={busy || clientIdInput.trim() === ""}
        >
          {busy ? "Connecting…" : "Connect"}
        </button>
      </div>
      {error && <p className="sp-error">{error}</p>}
    </div>
  );
}
