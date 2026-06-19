import { useCallback, useEffect, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { NavRail } from "./components/NavRail";
import { StatusBar } from "./components/StatusBar";
import { WaveformZone } from "./components/WaveformZone";
import { Deck } from "./components/Deck";
import { Mixer } from "./components/Mixer";
import { Library } from "./components/Library";
import { Instrument } from "./components/Instrument";
import { useDeck } from "./hooks/useDeck";
import { useAutoMix } from "./hooks/useAutoMix";
import { engineStatus, inTauri, onEngineLoad, onMasterMeter, setCrossfader, type EngineLoad, type MasterMeter } from "./lib/ipc";

const MAGENTA = "var(--accent)";
const CYAN = "var(--stream)";

export function App() {
  // Both decks are local/full-DSP for now (the engine supports two local decks). The
  // `dsp` flag drives the capability-locked treatment; flip a deck to dsp:false in
  // Phase 2 to render it as a streaming control-only deck.
  const deckA = useDeck(0, true);
  const deckB = useDeck(1, true);

  const [sampleRate, setSampleRate] = useState<number | null>(null);
  const [master, setMaster] = useState<MasterMeter>({ l: 0, r: 0 });
  const [load, setLoad] = useState<EngineLoad>({ load: 0, xruns: 0 });
  const [xfade, setXfade] = useState(0.5);
  const [showKeys, setShowKeys] = useState(false);

  const applyCrossfade = useCallback((v: number) => {
    setXfade(v);
    if (inTauri()) setCrossfader(v).catch(() => {});
  }, []);
  const auto = useAutoMix([deckA, deckB], applyCrossfade);

  useEffect(() => {
    if (!inTauri()) return;
    engineStatus().then((s) => setSampleRate(s.sample_rate)).catch(() => setSampleRate(0));
    const unMeter = onMasterMeter(setMaster);
    const unLoad = onEngineLoad(setLoad);
    return () => {
      unMeter.then((u) => u());
      unLoad.then((u) => u());
    };
  }, []);

  const masterBpm = deckA.state.meta ? deckA.state.meta.bpm * deckA.state.tempo : null;
  const loadedPaths = [deckA.state.meta?.path, deckB.state.meta?.path];

  const bothReady =
    !!deckA.state.meta && !!deckB.state.meta && deckA.state.meta.bpm > 0 && deckB.state.meta.bpm > 0;

  // Continuous beat-sync toggle: `target` follows the other deck (tempo + phase, held by the
  // engine PLL), or disengages. On engage we also match the displayed tempo; the engine refines
  // phase on top.
  const toggleSync = (target: "A" | "B") => {
    const t = target === "A" ? deckA : deckB;
    const s = target === "A" ? deckB : deckA;
    const masterIdx = target === "A" ? 1 : 0; // follow the other deck
    if (t.state.synced) {
      t.actions.sync(null);
      return;
    }
    if (!t.state.meta || !s.state.meta || t.state.meta.bpm <= 0 || s.state.meta.bpm <= 0) return;
    t.actions.setTempo((s.state.meta.bpm * s.state.tempo) / t.state.meta.bpm);
    t.actions.sync(masterIdx);
  };

  return (
    <div className="app">
      <TitleBar masterBpm={masterBpm} master={master} load={load} syncEnabled={bothReady} syncActive={deckB.state.synced} onSync={() => toggleSync("B")} keysOpen={showKeys} onToggleKeys={() => setShowKeys((v) => !v)} />
      <div className="body">
        <NavRail />
        <div className="content">
          <WaveformZone
            lanes={[
              { state: deckA.state, letter: "A", color: MAGENTA, onSeek: deckA.actions.seekFrac, onNudgeGrid: deckA.actions.nudgeGrid, onResetGrid: deckA.actions.resetGrid },
              { state: deckB.state, letter: "B", color: CYAN, onSeek: deckB.actions.seekFrac, onNudgeGrid: deckB.actions.nudgeGrid, onResetGrid: deckB.actions.resetGrid },
            ]}
          />
          <div className="deck-row">
            <Deck ctrl={deckA} color={MAGENTA} onSync={() => toggleSync("A")} syncEnabled={bothReady} syncActive={deckA.state.synced} />
            <Mixer
              channels={[
                { ctrl: deckA, letter: "A", color: MAGENTA },
                { ctrl: deckB, letter: "B", color: CYAN },
              ]}
              crossfader={xfade}
              onCrossfader={applyCrossfade}
              auto={{ enabled: auto.enabled, transitioning: auto.transitioning, onToggle: auto.toggle, onMixNow: auto.mixNow }}
            />
            <Deck ctrl={deckB} color={CYAN} onSync={() => toggleSync("B")} syncEnabled={bothReady} syncActive={deckB.state.synced} />
          </div>
          <Library loadedPaths={loadedPaths} />
        </div>
      </div>
      <StatusBar sampleRate={sampleRate} />
      {showKeys && <Instrument onClose={() => setShowKeys(false)} />}
    </div>
  );
}
