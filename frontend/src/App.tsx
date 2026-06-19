import { useCallback, useEffect, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { NavRail } from "./components/NavRail";
import { StatusBar } from "./components/StatusBar";
import { WaveformZone } from "./components/WaveformZone";
import { Deck } from "./components/Deck";
import { Mixer } from "./components/Mixer";
import { Library } from "./components/Library";
import { Instrument } from "./components/Instrument";
import { MidiMap } from "./components/MidiMap";
import { Sampler } from "./components/Sampler";
import { useDeck, type DeckController } from "./hooks/useDeck";
import { useAutoMix } from "./hooks/useAutoMix";
import { useCue } from "./hooks/useCue";
import { useMidi } from "./hooks/useMidi";
import { useMidiMap } from "./hooks/useMidiMap";
import { useSampler } from "./hooks/useSampler";
import { engineStatus, inTauri, onEngineLoad, onMasterMeter, setCrossfader, type EngineLoad, type MasterMeter } from "./lib/ipc";

const DECK_COLORS = ["var(--accent)", "var(--stream)", "var(--status-warn)", "var(--status-ok)"];
const DECK_LETTERS = ["A", "B", "C", "D"];

export function App() {
  // Four local/full-DSP decks; only two deck panels are shown at a time (switching slots),
  // while the mixer exposes all four channel strips.
  const deckA = useDeck(0, true);
  const deckB = useDeck(1, true);
  const deckC = useDeck(2, true);
  const deckD = useDeck(3, true);
  const decks = [deckA, deckB, deckC, deckD];

  const [sampleRate, setSampleRate] = useState<number | null>(null);
  const [master, setMaster] = useState<MasterMeter>({ l: 0, r: 0 });
  const [load, setLoad] = useState<EngineLoad>({ load: 0, xruns: 0 });
  const [xfade, setXfade] = useState(0.5);
  const [showKeys, setShowKeys] = useState(false);
  const [showMap, setShowMap] = useState(false);
  const [showPads, setShowPads] = useState(false);
  const midi = useMidi();
  const cue = useCue();
  const sampler = useSampler();
  // Which deck each on-screen slot controls: left ∈ {A,C}, right ∈ {B,D}.
  const [leftSel, setLeftSel] = useState(0);
  const [rightSel, setRightSel] = useState(1);
  const leftDeck = decks[leftSel];
  const rightDeck = decks[rightSel];

  const applyCrossfade = useCallback((v: number) => {
    setXfade(v);
    if (inTauri()) setCrossfader(v).catch(() => {});
  }, []);
  const auto = useAutoMix([leftDeck, rightDeck], applyCrossfade);

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

  const masterBpm = leftDeck.state.meta ? leftDeck.state.meta.bpm * leftDeck.state.tempo : null;
  const loadedPaths = decks.map((d) => d.state.meta?.path);

  // Two decks are sync-pairable when both visible slots are loaded with a tempo.
  const pairReady =
    !!leftDeck.state.meta && !!rightDeck.state.meta && leftDeck.state.meta.bpm > 0 && rightDeck.state.meta.bpm > 0;

  // Continuous beat-sync toggle: `target` follows the other visible deck (engine PLL), or
  // disengages. On engage we also match the displayed tempo; the engine refines phase on top.
  const toggleSync = (target: DeckController, source: DeckController) => {
    if (target.state.synced) {
      target.actions.sync(null);
      return;
    }
    if (!target.state.meta || !source.state.meta || target.state.meta.bpm <= 0 || source.state.meta.bpm <= 0) return;
    target.actions.setTempo((source.state.meta.bpm * source.state.tempo) / target.state.meta.bpm);
    target.actions.sync(source.deck);
  };

  // MIDI SYNC binds per deck index; only the two on-screen decks have a defined partner.
  const syncDeckByIndex = (i: number) => {
    if (i === leftSel) toggleSync(leftDeck, rightDeck);
    else if (i === rightSel) toggleSync(rightDeck, leftDeck);
  };
  const midiMap = useMidiMap(decks, {
    crossfader: applyCrossfade,
    syncDeck: syncDeckByIndex,
    deckCue: cue.toggleDeckCue,
    samplerPad: sampler.trigger,
  });

  const slotLane = (d: DeckController) => ({
    state: d.state,
    letter: DECK_LETTERS[d.deck],
    color: DECK_COLORS[d.deck],
    onSeek: d.actions.seekFrac,
    onNudgeGrid: d.actions.nudgeGrid,
    onResetGrid: d.actions.resetGrid,
  });

  return (
    <div className="app">
      <TitleBar masterBpm={masterBpm} master={master} load={load} syncEnabled={pairReady} syncActive={rightDeck.state.synced} onSync={() => toggleSync(rightDeck, leftDeck)} keysOpen={showKeys} onToggleKeys={() => setShowKeys((v) => !v)} mapOpen={showMap} onToggleMap={() => setShowMap((v) => !v)} padsOpen={showPads} onTogglePads={() => setShowPads((v) => !v)} />
      <div className="body">
        <NavRail />
        <div className="content">
          <WaveformZone lanes={[slotLane(leftDeck), slotLane(rightDeck)]} />
          <div className="deck-row">
            <Deck
              ctrl={leftDeck}
              color={DECK_COLORS[leftDeck.deck]}
              onSync={() => toggleSync(leftDeck, rightDeck)}
              syncEnabled={pairReady}
              syncActive={leftDeck.state.synced}
              mirror
              slots={[
                { label: "A", active: leftSel === 0, onSelect: () => setLeftSel(0) },
                { label: "C", active: leftSel === 2, onSelect: () => setLeftSel(2) },
              ]}
            />
            <Mixer
              channels={decks.map((d) => ({ ctrl: d, letter: DECK_LETTERS[d.deck], color: DECK_COLORS[d.deck] }))}
              crossfader={xfade}
              onCrossfader={applyCrossfade}
              auto={{ enabled: auto.enabled, transitioning: auto.transitioning, onToggle: auto.toggle, onMixNow: auto.mixNow }}
              cue={cue}
            />
            <Deck
              ctrl={rightDeck}
              color={DECK_COLORS[rightDeck.deck]}
              onSync={() => toggleSync(rightDeck, leftDeck)}
              syncEnabled={pairReady}
              syncActive={rightDeck.state.synced}
              slots={[
                { label: "B", active: rightSel === 1, onSelect: () => setRightSel(1) },
                { label: "D", active: rightSel === 3, onSelect: () => setRightSel(3) },
              ]}
            />
          </div>
          <Library loadedPaths={loadedPaths} />
        </div>
      </div>
      <StatusBar sampleRate={sampleRate} />
      {showKeys && <Instrument midi={midi} onClose={() => setShowKeys(false)} />}
      {showMap && <MidiMap midi={midi} map={midiMap} onClose={() => setShowMap(false)} />}
      {showPads && <Sampler sampler={sampler} onClose={() => setShowPads(false)} />}
    </div>
  );
}
