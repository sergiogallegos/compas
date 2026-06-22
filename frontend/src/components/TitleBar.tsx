import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "./Logo";
import { Icon } from "./icons";
import { buildInfo, checkForUpdate, inTauri, type BuildInfo, type EngineLoad, type MasterMeter } from "../lib/ipc";

export function TitleBar({
  masterBpm,
  master,
  load,
  syncEnabled = false,
  syncActive = false,
  onSync,
  keysOpen = false,
  onToggleKeys,
  mapOpen = false,
  onToggleMap,
  padsOpen = false,
  onTogglePads,
  controllersOpen = false,
  onToggleControllers,
  recording = false,
  recBusy = false,
  onToggleRecord,
  onOpenSettings,
  onOpenProfile,
  profileInitial = "M",
}: {
  masterBpm: number | null;
  master: MasterMeter;
  load?: EngineLoad;
  syncEnabled?: boolean;
  syncActive?: boolean;
  onSync?: () => void;
  keysOpen?: boolean;
  onToggleKeys?: () => void;
  mapOpen?: boolean;
  onToggleMap?: () => void;
  padsOpen?: boolean;
  onTogglePads?: () => void;
  controllersOpen?: boolean;
  onToggleControllers?: () => void;
  recording?: boolean;
  recBusy?: boolean;
  onToggleRecord?: () => void;
  onOpenSettings?: () => void;
  onOpenProfile?: () => void;
  profileInitial?: string;
}) {
  const bar = (v: number) => `${Math.min(100, Math.sqrt(Math.max(0, v)) * 100)}%`;
  const win = () => (inTauri() ? getCurrentWindow() : null);

  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [updBusy, setUpdBusy] = useState(false);
  const [metronomeOn, setMetronomeOn] = useState(false);
  const metronomeBeat = useRef(0);
  useEffect(() => {
    if (!inTauri()) return;
    buildInfo().then(setBuild).catch(() => setBuild(null));
  }, []);
  useEffect(() => {
    if (!metronomeOn) return;
    let ctx: AudioContext | null = null;
    const bpm = masterBpm && masterBpm > 0 ? masterBpm : 120;
    const ms = Math.max(125, 60_000 / bpm);
    const click = () => {
      ctx ??= new AudioContext();
      const now = ctx.currentTime;
      const accent = metronomeBeat.current === 0;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = accent ? 1320 : 880;
      gain.gain.setValueAtTime(accent ? 0.08 : 0.045, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.035);
      osc.connect(gain).connect(ctx.destination);
      osc.start(now);
      osc.stop(now + 0.04);
      metronomeBeat.current = (metronomeBeat.current + 1) % 4;
    };
    click();
    const timer = window.setInterval(click, ms);
    return () => {
      window.clearInterval(timer);
      ctx?.close().catch(() => {});
      metronomeBeat.current = 0;
    };
  }, [masterBpm, metronomeOn]);
  const onCheckUpdate = async () => {
    if (!inTauri() || updBusy) return;
    setUpdBusy(true);
    try {
      await checkForUpdate();
    } finally {
      setUpdBusy(false);
    }
  };
  return (
    <header className="titlebar" data-tauri-drag-region>
      {/* macOS-style controls: close / minimize / zoom */}
      <div className="lights">
        <button className="light" style={{ background: "#ff5f57" }} title="Close" onClick={() => win()?.close()} />
        <button className="light" style={{ background: "#febc2e" }} title="Minimize" onClick={() => win()?.minimize()} />
        <button className="light" style={{ background: "#28c840" }} title="Maximize" onClick={() => win()?.toggleMaximize()} />
      </div>
      <div className="brand" data-tauri-drag-region>
        <Logo size={22} />
        <span className="brand-word display">compas</span>
      </div>

      <div className="master-cluster" data-tauri-drag-region>
        <div className="master-pill">
          <div className="master-bpm">
            <span className="overline">MASTER</span>
            <span className="mono master-bpm-val">
              {masterBpm ? masterBpm.toFixed(1) : "—"}
              <small>BPM</small>
            </span>
          </div>
          <button
            className={`sync-chip ${syncActive ? "sync-chip--on" : ""}`}
            onClick={onSync}
            disabled={!syncEnabled && !syncActive}
            title="Continuous beat-sync: Deck B follows Deck A (tempo + phase)"
          >
            <Icon name="link" size={12} /> SYNC
          </button>
          <div className="master-meter">
            <div className="mm-bar"><div className="mm-fill" style={{ height: bar(master.l) }} /></div>
            <div className="mm-bar"><div className="mm-fill" style={{ height: bar(master.r) }} /></div>
          </div>
        </div>
        <div className="mini-transport">
          <button
            className={`mt-btn ${metronomeOn ? "mt-rec--on" : ""}`}
            onClick={() => setMetronomeOn((v) => !v)}
            title={metronomeOn ? "Stop metronome" : "Start metronome"}
          >
            <Icon name="play" size={13} />
          </button>
          <span className="mt-btn mono">4/4</span>
          <button
            className={`mt-btn ${keysOpen ? "mt-rec--on" : ""}`}
            onClick={onToggleKeys}
            title="Synth instrument keyboard"
          >
            <Icon name="music" size={13} />
          </button>
          <button
            className={`mt-btn ${mapOpen ? "mt-rec--on" : ""}`}
            onClick={onToggleMap}
            title="MIDI controller mapping (learn)"
          >
            <Icon name="sliders" size={13} />
          </button>
          <button
            className={`mt-btn ${padsOpen ? "mt-rec--on" : ""}`}
            onClick={onTogglePads}
            title="Sampler / performance pads"
          >
            <Icon name="pads" size={13} />
          </button>
          <button
            className={`mt-btn ${controllersOpen ? "mt-rec--on" : ""}`}
            onClick={onToggleControllers}
            title="Controller profiles (mapping & learn)"
          >
            <Icon name="sliders" size={13} />
          </button>
          <button
            className={`mt-btn mt-rec ${recording ? "mt-rec--on" : ""}`}
            onClick={onToggleRecord}
            disabled={recBusy}
            title={recording ? "Stop recording the master mix" : "Record the master mix to a WAV"}
          >
            <span className="rec-dot" />
          </button>
        </div>
      </div>

      <div className="titlebar-right">
        {(() => {
          const pct = Math.round((load?.load ?? 0) * 100);
          const xruns = load?.xruns ?? 0;
          const commandDrops = load?.command_ring_full ?? 0;
          const recordDrops = load?.record_ring_drops ?? 0;
          const cueDrops = load?.cue_ring_drops ?? 0;
          const reclaimPressure = load?.reclaim_ring_full ?? 0;
          const drops = commandDrops + recordDrops + cueDrops;
          const pressure = drops + reclaimPressure;
          const tone = xruns > 0 || pressure > 0 || pct >= 100 ? "var(--status-alarm-2)" : pct >= 70 ? "var(--status-warn)" : "var(--status-ok)";
          const text = xruns > 0 || pressure > 0 ? `RT ⚠ ${xruns + pressure}` : `RT ${pct}%`;
          return (
            <span className="mono cpu" style={{ color: tone }} title={`Audio-thread load ${pct}% · callback overruns ${xruns} · command drops ${commandDrops} · record drops ${recordDrops} · cue drops ${cueDrops} · reclaim pressure ${reclaimPressure}`}>
              {text}
            </span>
          );
        })()}
        {build && (
          <button
            className="mono build-chip"
            onClick={onCheckUpdate}
            disabled={updBusy}
            title={`compas ${build.version} · ${build.sha}${
              build.built_at ? ` · built ${new Date(Number(build.built_at) * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC` : ""
            } · click to check for updates`}
          >
            {updBusy ? "…" : `v${build.version} · ${build.sha}`}
          </button>
        )}
        <button className="icon-btn" onClick={onOpenSettings} title="Settings"><Icon name="settings" size={16} /></button>
        <button className="avatar" onClick={onOpenProfile} title="Profile">{profileInitial}</button>
      </div>
    </header>
  );
}
