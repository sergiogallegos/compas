import { useEffect, useState } from "react";
import { appInfo, inTauri, setCrossfader, setMasterGain, type AppInfo } from "./lib/ipc";

/**
 * P0 scaffold UI. Proves the IPC bridge (app_info, crossfader, master gain) and
 * sketches the dual-deck layout. No real decks/waveforms yet — that is Phase 1.
 */
export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [xfade, setXfade] = useState(0.5);
  const [master, setMaster] = useState(0.85);

  useEffect(() => {
    if (!inTauri()) {
      setErr("Running outside Tauri (browser dev). Native commands are unavailable.");
      return;
    }
    appInfo().then(setInfo).catch((e) => setErr(String(e)));
  }, []);

  return (
    <div className="app">
      <header className="topbar">
        <h1>compas</h1>
        <span className="phase">
          {info ? `${info.name} v${info.version} · ${info.phase}` : err ?? "connecting…"}
        </span>
      </header>

      <main className="decks">
        <DeckPanel label="Deck A" kind="local" />
        <section className="mixer">
          <label className="knob">
            Master
            <input
              type="range" min={0} max={1} step={0.01} value={master}
              onChange={(e) => {
                const v = Number(e.target.value);
                setMaster(v);
                if (inTauri()) void setMasterGain(v).catch(() => {});
              }}
            />
          </label>
          <label className="knob">
            Crossfader
            <input
              type="range" min={0} max={1} step={0.01} value={xfade}
              onChange={(e) => {
                const v = Number(e.target.value);
                setXfade(v);
                if (inTauri()) void setCrossfader(v).catch(() => {});
              }}
            />
          </label>
        </section>
        <DeckPanel label="Deck B" kind="streaming" />
      </main>

      <footer className="status">
        Waveforms, BPM/key, beatmatch arrive in Phase 1. Streaming decks (Phase 2) are
        control-only and will visibly disable DSP they can't perform.
      </footer>
    </div>
  );
}

function DeckPanel({ label, kind }: { label: string; kind: "local" | "streaming" }) {
  const streaming = kind === "streaming";
  return (
    <section className={`deck ${streaming ? "deck--streaming" : ""}`}>
      <h2>{label}</h2>
      <div className="platter" aria-hidden />
      <div className="waveform-placeholder">waveform (P1)</div>
      <div className="deck-controls">
        <button disabled>▶ play</button>
        <button disabled title={streaming ? "Not available for streaming sources" : undefined}>
          EQ {streaming ? "🔒" : ""}
        </button>
        <button disabled title={streaming ? "Not available for streaming sources" : undefined}>
          sync {streaming ? "🔒" : ""}
        </button>
      </div>
      {streaming && (
        <p className="cap-note">
          Streaming source: playback control only — no EQ, filter, sync, or scratch.
        </p>
      )}
    </section>
  );
}
