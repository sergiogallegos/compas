import { useCallback, useEffect, useRef, useState } from "react";
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
import { engineStatus, inTauri, onControllerUpdate, onEngineLoad, onMasterMeter, setCrossfader, setCrossfaderConfig, setDeckFxMacro, setMasterGain, type ControllerUpdate, type EngineLoad, type MasterMeter } from "./lib/ipc";

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

  // Crossfader response config (curve steepness, additive/cut mode, reverse).
  const [xfCurve, setXfCurve] = useState(1);
  const [xfAdditive, setXfAdditive] = useState(false);
  const [xfReverse, setXfReverse] = useState(false);
  const applyXfConfig = useCallback((curve: number, additive: boolean, reverse: boolean) => {
    setXfCurve(curve);
    setXfAdditive(additive);
    setXfReverse(reverse);
    if (inTauri()) setCrossfaderConfig(curve, additive ? 1 : 0, reverse).catch(() => {});
  }, []);
  const auto = useAutoMix([leftDeck, rightDeck], applyCrossfade);

  const applyFxMacro = useCallback((deck: number, v: number) => {
    if (inTauri()) setDeckFxMacro(deck, v).catch(() => {});
  }, []);

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

  // Controller bus: apply resolved controller:update events through the existing setters. A ref
  // keeps the handler current without re-subscribing the listener on every render.
  const dispatchRef = useRef<(u: ControllerUpdate) => void>(() => {});
  dispatchRef.current = (u: ControllerUpdate) => {
    const { control, value } = u;
    if (control === "mixer.crossfader") return applyCrossfade(value);
    if (control === "mixer.master_gain") {
      setMasterGain(value).catch(() => {});
      return;
    }
    const m = control.match(/^deck\.(\d+)\.(.+)$/);
    if (!m) return;
    const d = decks[parseInt(m[1], 10)];
    if (!d) return;
    const { actions: a, state: s } = d;
    switch (m[2]) {
      case "gain": a.setGain(value); break;
      case "filter": a.setFilter(value); break;
      case "tempo": a.setTempo(1 + value / 100); break;
      case "eq.low": a.setEq({ ...s.eq, low: value }); break;
      case "eq.mid": a.setEq({ ...s.eq, mid: value }); break;
      case "eq.high": a.setEq({ ...s.eq, hi: value }); break;
      case "play": value >= 0.5 ? a.play() : a.pause(); break;
      case "cue": a.cueButton(value >= 0.5); break;
      case "keylock": if (value >= 0.5) a.toggleKeylock(); break;
      case "sync": if (value >= 0.5) syncDeckByIndex(parseInt(m[1], 10)); break;
    }
  };
  useEffect(() => {
    if (!inTauri()) return;
    const un = onControllerUpdate((u) => dispatchRef.current(u));
    return () => {
      un.then((u) => u());
    };
  }, []);

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
              xfader={{ curve: xfCurve, additive: xfAdditive, reverse: xfReverse, onChange: applyXfConfig }}
              onFxMacro={applyFxMacro}
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
