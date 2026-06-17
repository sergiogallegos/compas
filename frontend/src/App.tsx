import { useEffect, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { NavRail } from "./components/NavRail";
import { StatusBar } from "./components/StatusBar";
import { WaveformZone } from "./components/WaveformZone";
import { Deck } from "./components/Deck";
import { Mixer } from "./components/Mixer";
import { Library, type LibRow } from "./components/Library";
import { useDeck } from "./hooks/useDeck";
import { engineStatus, inTauri, onMasterMeter, setCrossfader, type MasterMeter } from "./lib/ipc";

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
  const [xfade, setXfade] = useState(0.5);

  useEffect(() => {
    if (!inTauri()) return;
    engineStatus().then((s) => setSampleRate(s.sample_rate)).catch(() => setSampleRate(0));
    const un = onMasterMeter(setMaster);
    return () => {
      un.then((u) => u());
    };
  }, []);

  const masterBpm = deckA.state.meta ? deckA.state.meta.bpm * deckA.state.tempo : null;

  const libRows: LibRow[] = [
    deckA.state.meta && { letter: "A", color: MAGENTA, meta: deckA.state.meta },
    deckB.state.meta && { letter: "B", color: CYAN, meta: deckB.state.meta },
  ].filter(Boolean) as LibRow[];

  return (
    <div className="app">
      <TitleBar masterBpm={masterBpm} master={master} />
      <div className="body">
        <NavRail />
        <div className="content">
          <WaveformZone
            lanes={[
              { state: deckA.state, letter: "A", color: MAGENTA, onSeek: deckA.actions.seekFrac },
              { state: deckB.state, letter: "B", color: CYAN, onSeek: deckB.actions.seekFrac },
            ]}
          />
          <div className="deck-row">
            <Deck ctrl={deckA} color={MAGENTA} />
            <Mixer
              channels={[
                { ctrl: deckA, letter: "A", color: MAGENTA },
                { ctrl: deckB, letter: "B", color: CYAN },
              ]}
              crossfader={xfade}
              onCrossfader={(v) => {
                setXfade(v);
                if (inTauri()) setCrossfader(v).catch(() => {});
              }}
            />
            <Deck ctrl={deckB} color={CYAN} />
          </div>
          <Library rows={libRows} />
        </div>
      </div>
      <StatusBar sampleRate={sampleRate} />
    </div>
  );
}
