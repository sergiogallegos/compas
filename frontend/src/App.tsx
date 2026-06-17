import { useEffect, useState } from "react";
import { Deck } from "./components/Deck";
import { engineStatus, inTauri, setCrossfader, setMasterGain } from "./lib/ipc";

/**
 * Phase 1 shell: two local-file decks + a center mixer (crossfader + master).
 * Streaming decks (Phase 2) will appear alongside these and disable the DSP they can't do.
 */
export function App() {
  const [sampleRate, setSampleRate] = useState<number | null>(null);
  const [browser, setBrowser] = useState(false);
  const [xfade, setXfade] = useState(0.5);
  const [master, setMaster] = useState(0.85);

  useEffect(() => {
    if (!inTauri()) {
      setBrowser(true);
      return;
    }
    engineStatus()
      .then((s) => setSampleRate(s.sample_rate))
      .catch(() => setSampleRate(0));
  }, []);

  return (
    <div className="app">
      <header className="topbar">
        <h1>compas</h1>
        <span className="phase">
          {browser
            ? "browser dev — native engine unavailable"
            : sampleRate
              ? `engine @ ${sampleRate} Hz · P1 local dual-deck`
              : sampleRate === 0
                ? "no audio device — UI only"
                : "connecting…"}
        </span>
      </header>

      <main className="decks">
        <Deck deck={0} side="A" />

        <section className="mixer">
          <label className="ctrl vertical">
            Master
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={master}
              onChange={(e) => {
                const v = Number(e.target.value);
                setMaster(v);
                if (inTauri()) setMasterGain(v).catch(() => {});
              }}
            />
          </label>
          <label className="ctrl">
            Crossfader
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={xfade}
              onChange={(e) => {
                const v = Number(e.target.value);
                setXfade(v);
                if (inTauri()) setCrossfader(v).catch(() => {});
              }}
            />
            <span className="xf-labels">
              <span>A</span>
              <span>B</span>
            </span>
          </label>
        </section>

        <Deck deck={1} side="B" />
      </main>

      <footer className="status">
        Load two tracks, match BPMs with the tempo faders (nudge ± to align phase), and ride the
        crossfader. Varispeed (vinyl-style) is the default; key-lock arrives later in P1.
      </footer>
    </div>
  );
}
