import { useCallback, useEffect, useRef, useState } from "react";
import { TitleBar } from "./components/TitleBar";
import { StatusBar } from "./components/StatusBar";
import { WaveformZone } from "./components/WaveformZone";
import { Deck } from "./components/Deck";
import { Mixer } from "./components/Mixer";
import { Library } from "./components/Library";
import { Instrument } from "./components/Instrument";
import { MidiMap } from "./components/MidiMap";
import { ControllerMap } from "./components/ControllerMap";
import { Sampler } from "./components/Sampler";
import { SettingsPanel } from "./components/SettingsPanel";
import { ProfilePanel } from "./components/ProfilePanel";
import { useDeck, type DeckController } from "./hooks/useDeck";
import { useAutoMix } from "./hooks/useAutoMix";
import { useCue } from "./hooks/useCue";
import { useBooth } from "./hooks/useBooth";
import { useAux } from "./hooks/useAux";
import { useInternalClock } from "./hooks/useInternalClock";
import { useMidi } from "./hooks/useMidi";
import { useMidiMap } from "./hooks/useMidiMap";
import { useSampler } from "./hooks/useSampler";
import { controllerFeedback, engineStatus, inTauri, onControllerUpdate, onEngineLoad, onMasterMeter, pickRecordingPath, setCrossfader, setCrossfaderConfig, setDeckFxMacro, setMasterGain, startRecording, stopRecording, type ControllerUpdate, type EngineLoad, type EngineStatus, type KeyNotation, type MasterMeter } from "./lib/ipc";

const DECK_COLORS = ["var(--deck-a)", "var(--deck-b)", "var(--deck-c)", "var(--deck-d)"];
const DECK_LETTERS = ["A", "B", "C", "D"];

export function App() {
  // Four local/full-DSP decks; only two deck panels are shown at a time (switching slots),
  // while the mixer exposes all four channel strips.
  // The internal master clock is a global tempo source decks can follow (INT) and beat-synced FX
  // track; created before the decks so each useDeck can read its tempo.
  const internalClock = useInternalClock();
  const deckA = useDeck(0, true, internalClock);
  const deckB = useDeck(1, true, internalClock);
  const deckC = useDeck(2, true, internalClock);
  const deckD = useDeck(3, true, internalClock);
  const decks = [deckA, deckB, deckC, deckD];

  const [sampleRate, setSampleRate] = useState<number | null>(null);
  const [audioStatus, setAudioStatus] = useState<Pick<EngineStatus, "audio_online" | "audio_restarting" | "audio_restarts" | "audio_error" | "cue_device_latency_secs" | "cue_prime_latency_secs" | "booth_device_latency_secs" | "booth_prime_latency_secs"> | null>(null);
  const [master, setMaster] = useState<MasterMeter>({ l: 0, r: 0 });
  const [load, setLoad] = useState<EngineLoad>({
    load: 0,
    xruns: 0,
    command_ring_full: 0,
    record_ring_drops: 0,
    cue_ring_drops: 0,
    reclaim_ring_full: 0,
  });
  const [xfade, setXfade] = useState(0.5);
  const [showKeys, setShowKeys] = useState(false);
  const [showMap, setShowMap] = useState(false);
  const [showPads, setShowPads] = useState(false);
  const [showControllers, setShowControllers] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showProfile, setShowProfile] = useState(false);
  const [contrast, setContrast] = useState(false);
  const [keyNotation, setKeyNotation] = useState<KeyNotation>(
    () => (localStorage.getItem("compas.keyNotation") as KeyNotation | null) ?? "camelot",
  );
  useEffect(() => {
    localStorage.setItem("compas.keyNotation", keyNotation);
  }, [keyNotation]);
  const [deckCount, setDeckCount] = useState<2 | 4>(() =>
    localStorage.getItem("compas.deckCount") === "4" ? 4 : 2,
  );
  useEffect(() => {
    localStorage.setItem("compas.deckCount", String(deckCount));
  }, [deckCount]);
  const [recording, setRecording] = useState(false);
  const [recBusy, setRecBusy] = useState(false);
  const [profileName, setProfileName] = useState(() => localStorage.getItem("compas.profileName") ?? "Main");
  const midi = useMidi();
  const cue = useCue();
  const booth = useBooth();
  const aux = useAux();
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
    let alive = true;
    const refreshStatus = () => {
      engineStatus()
        .then((s) => {
          if (!alive) return;
          setSampleRate(s.sample_rate);
          setAudioStatus({
            audio_online: s.audio_online,
            audio_restarting: s.audio_restarting,
            audio_restarts: s.audio_restarts,
            audio_error: s.audio_error,
            cue_device_latency_secs: s.cue_device_latency_secs,
            cue_prime_latency_secs: s.cue_prime_latency_secs,
            booth_device_latency_secs: s.booth_device_latency_secs,
            booth_prime_latency_secs: s.booth_prime_latency_secs,
          });
        })
        .catch(() => {
          if (!alive) return;
          setSampleRate(0);
          setAudioStatus({
            audio_online: false,
            audio_restarting: false,
            audio_restarts: 0,
            audio_error: "engine status unavailable",
            cue_device_latency_secs: 0,
            cue_prime_latency_secs: 0,
            booth_device_latency_secs: 0,
            booth_prime_latency_secs: 0,
          });
        });
    };
    refreshStatus();
    const statusTimer = window.setInterval(refreshStatus, 2000);
    const unMeter = onMasterMeter(setMaster);
    const unLoad = onEngineLoad(setLoad);
    return () => {
      alive = false;
      window.clearInterval(statusTimer);
      unMeter.then((u) => u());
      unLoad.then((u) => u());
    };
  }, []);

  const masterBpm = leftDeck.state.meta ? leftDeck.state.meta.bpm * leftDeck.state.tempo : null;
  const loadedPaths = decks.map((d) => d.state.meta?.path);
  const profileInitial = profileName.trim().charAt(0).toUpperCase() || "M";

  useEffect(() => {
    localStorage.setItem("compas.profileName", profileName);
  }, [profileName]);

  const toggleRecord = useCallback(async () => {
    if (!inTauri() || recBusy) return;
    setRecBusy(true);
    try {
      if (recording) {
        await stopRecording();
        setRecording(false);
      } else {
        const path = await pickRecordingPath();
        if (path) {
          await startRecording(path);
          setRecording(true);
        }
      }
    } catch {
      setRecording(false);
    } finally {
      setRecBusy(false);
    }
  }, [recBusy, recording]);

  const openPanel = (setter: (open: boolean) => void) => {
    setter(true);
    setShowSettings(false);
  };

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
    if (control === "sampler.gain") {
      sampler.setGain(value);
      return;
    }
    const sm = control.match(/^sampler\.(\d+)\.trigger$/);
    if (sm) {
      if (value >= 0.5) sampler.trigger(parseInt(sm[1], 10));
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
      case "play": if (value >= 0.5) a.togglePlay(); break; // hardware buttons latch on press
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

  // LED/motor feedback: push each mapped control's current value to the device so the hardware
  // tracks software state (lit pads, LED rings, motor faders) — for UI changes, not just the
  // controller's own moves. The engine no-ops without an active profile/output and dedups
  // redundant resends. Tempo state is a ratio; the control bus speaks ±percent. (Master gain has
  // no UI control, so it rides the controller-echo path only.)
  const pushFeedback = useCallback(() => {
    if (!inTauri()) return;
    for (const d of decks) {
      const s = d.state;
      controllerFeedback(`deck.${d.deck}.gain`, s.gain).catch(() => {});
      controllerFeedback(`deck.${d.deck}.filter`, s.filter).catch(() => {});
      controllerFeedback(`deck.${d.deck}.eq.low`, s.eq.low).catch(() => {});
      controllerFeedback(`deck.${d.deck}.eq.mid`, s.eq.mid).catch(() => {});
      controllerFeedback(`deck.${d.deck}.eq.high`, s.eq.hi).catch(() => {});
      controllerFeedback(`deck.${d.deck}.tempo`, (s.tempo - 1) * 100).catch(() => {});
      controllerFeedback(`deck.${d.deck}.play`, s.playing ? 1 : 0).catch(() => {});
      controllerFeedback(`deck.${d.deck}.keylock`, s.keylock ? 1 : 0).catch(() => {});
      controllerFeedback(`deck.${d.deck}.sync`, s.synced ? 1 : 0).catch(() => {});
    }
    controllerFeedback("mixer.crossfader", xfade).catch(() => {});
    controllerFeedback("sampler.gain", sampler.gain).catch(() => {});
    // Depend on the mapped scalars only (not the `decks` array, which is a new identity each
    // render, nor frame/level which tick at 30 Hz) so this re-runs only on real control changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    deckA.state.gain, deckA.state.filter, deckA.state.eq.low, deckA.state.eq.mid, deckA.state.eq.hi, deckA.state.tempo, deckA.state.playing, deckA.state.keylock, deckA.state.synced,
    deckB.state.gain, deckB.state.filter, deckB.state.eq.low, deckB.state.eq.mid, deckB.state.eq.hi, deckB.state.tempo, deckB.state.playing, deckB.state.keylock, deckB.state.synced,
    deckC.state.gain, deckC.state.filter, deckC.state.eq.low, deckC.state.eq.mid, deckC.state.eq.hi, deckC.state.tempo, deckC.state.playing, deckC.state.keylock, deckC.state.synced,
    deckD.state.gain, deckD.state.filter, deckD.state.eq.low, deckD.state.eq.mid, deckD.state.eq.hi, deckD.state.tempo, deckD.state.playing, deckD.state.keylock, deckD.state.synced,
    xfade, sampler.gain,
  ]);

  // Re-push on any mapped value change…
  useEffect(() => {
    pushFeedback();
  }, [pushFeedback]);
  // …and on demand when a profile is (re)activated, to sync the device to current state at connect.
  useEffect(() => {
    const onResync = () => pushFeedback();
    window.addEventListener("controller:resync", onResync);
    return () => window.removeEventListener("controller:resync", onResync);
  }, [pushFeedback]);

  const slotLane = (d: DeckController) => ({
    state: d.state,
    letter: DECK_LETTERS[d.deck],
    color: DECK_COLORS[d.deck],
    onSeek: d.actions.seekFrac,
    onNudgeGrid: d.actions.nudgeGrid,
    onResetGrid: d.actions.resetGrid,
  });

  const quad = deckCount === 4;
  // In 4-deck mode each deck syncs to the other deck in its row (A↔B, C↔D; partner index = deck ^ 1).
  const fourDeckProps = (d: DeckController) => {
    const partner = decks[d.deck ^ 1];
    const ready =
      !!d.state.meta && !!partner.state.meta && d.state.meta.bpm > 0 && partner.state.meta.bpm > 0;
    return {
      ctrl: d,
      color: DECK_COLORS[d.deck],
      onSync: () => toggleSync(d, partner),
      syncEnabled: ready,
      syncActive: d.state.synced,
      keyNotation,
      compact: true,
    };
  };
  // Mixer props shared by both layouts; only the visible channel set differs (2 strips vs 4).
  const mixerCommon = {
    crossfader: xfade,
    onCrossfader: applyCrossfade,
    xfader: { curve: xfCurve, additive: xfAdditive, reverse: xfReverse, onChange: applyXfConfig },
    onFxMacro: applyFxMacro,
    auto: { enabled: auto.enabled, transitioning: auto.transitioning, onToggle: auto.toggle, onMixNow: auto.mixNow },
    cue,
  };
  const channelOf = (d: DeckController) => ({ ctrl: d, letter: DECK_LETTERS[d.deck], color: DECK_COLORS[d.deck] });

  return (
    <div className="app" data-contrast={contrast ? "high" : "standard"}>
      <TitleBar masterBpm={masterBpm} master={master} load={load} syncEnabled={pairReady} syncActive={rightDeck.state.synced} onSync={() => toggleSync(rightDeck, leftDeck)} keysOpen={showKeys} onToggleKeys={() => setShowKeys((v) => !v)} mapOpen={showMap} onToggleMap={() => setShowMap((v) => !v)} padsOpen={showPads} onTogglePads={() => setShowPads((v) => !v)} controllersOpen={showControllers} onToggleControllers={() => setShowControllers((v) => !v)} recording={recording} recBusy={recBusy} onToggleRecord={toggleRecord} onOpenSettings={() => setShowSettings(true)} onOpenProfile={() => setShowProfile(true)} profileInitial={profileInitial} contrast={contrast} onToggleContrast={() => setContrast((v) => !v)} />
      <div className="body">
        <div className="content">
          {quad ? (
            <>
              <WaveformZone lanes={[slotLane(deckA), slotLane(deckB), slotLane(deckC), slotLane(deckD)]} />
              <div className="deck-row deck-row--quad">
                <div className="deck-col">
                  <Deck {...fourDeckProps(deckA)} mirror />
                  <Deck {...fourDeckProps(deckC)} mirror />
                </div>
                <Mixer channels={decks.map(channelOf)} {...mixerCommon} quad />
                <div className="deck-col">
                  <Deck {...fourDeckProps(deckB)} />
                  <Deck {...fourDeckProps(deckD)} />
                </div>
              </div>
            </>
          ) : (
            <>
              <WaveformZone lanes={[slotLane(leftDeck), slotLane(rightDeck)]} />
              <div className="deck-row">
                <Deck
                  ctrl={leftDeck}
                  color={DECK_COLORS[leftDeck.deck]}
                  onSync={() => toggleSync(leftDeck, rightDeck)}
                  syncEnabled={pairReady}
                  syncActive={leftDeck.state.synced}
                  keyNotation={keyNotation}
                  mirror
                  slots={[
                    { label: "A", active: leftSel === 0, onSelect: () => setLeftSel(0) },
                    { label: "C", active: leftSel === 2, onSelect: () => setLeftSel(2) },
                  ]}
                />
                <Mixer channels={[leftDeck, rightDeck].map(channelOf)} {...mixerCommon} />
                <Deck
                  ctrl={rightDeck}
                  color={DECK_COLORS[rightDeck.deck]}
                  onSync={() => toggleSync(rightDeck, leftDeck)}
                  syncEnabled={pairReady}
                  syncActive={rightDeck.state.synced}
                  keyNotation={keyNotation}
                  slots={[
                    { label: "B", active: rightSel === 1, onSelect: () => setRightSel(1) },
                    { label: "D", active: rightSel === 3, onSelect: () => setRightSel(3) },
                  ]}
                />
              </div>
            </>
          )}
          <Library loadedPaths={loadedPaths} keyNotation={keyNotation} />
        </div>
      </div>
      <StatusBar sampleRate={sampleRate} audioStatus={audioStatus} />
      {showKeys && <Instrument midi={midi} onClose={() => setShowKeys(false)} />}
      {showMap && <MidiMap midi={midi} map={midiMap} onClose={() => setShowMap(false)} />}
      {showPads && <Sampler sampler={sampler} onClose={() => setShowPads(false)} />}
      {showControllers && <ControllerMap onClose={() => setShowControllers(false)} />}
      {showSettings && (
        <SettingsPanel
          aux={aux}
          booth={booth}
          cue={cue}
          clock={internalClock}
          contrast={contrast}
          onToggleContrast={() => setContrast((v) => !v)}
          keyNotation={keyNotation}
          onToggleKeyNotation={() => setKeyNotation((v) => (v === "camelot" ? "musical" : "camelot"))}
          deckCount={deckCount}
          onToggleDeckCount={() => setDeckCount((v) => (v === 2 ? 4 : 2))}
          onOpenKeys={() => openPanel(setShowKeys)}
          onOpenMap={() => openPanel(setShowMap)}
          onOpenPads={() => openPanel(setShowPads)}
          onOpenControllers={() => openPanel(setShowControllers)}
          onClose={() => setShowSettings(false)}
        />
      )}
      {showProfile && (
        <ProfilePanel
          name={profileName}
          onName={setProfileName}
          onClose={() => setShowProfile(false)}
        />
      )}
    </div>
  );
}
