import { useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "./Logo";
import { Icon } from "./icons";
import { inTauri, pickRecordingPath, startRecording, stopRecording, type EngineLoad, type MasterMeter } from "../lib/ipc";

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
}) {
  const bar = (v: number) => `${Math.min(100, Math.sqrt(Math.max(0, v)) * 100)}%`;
  const win = () => (inTauri() ? getCurrentWindow() : null);

  const [recording, setRecording] = useState(false);
  const [recBusy, setRecBusy] = useState(false);
  const toggleRecord = async () => {
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
      // Failed to start/stop — reset to a safe state.
      setRecording(false);
    } finally {
      setRecBusy(false);
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
          <button className="mt-btn" disabled title="Metronome: later"><Icon name="play" size={13} /></button>
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
            className={`mt-btn mt-rec ${recording ? "mt-rec--on" : ""}`}
            onClick={toggleRecord}
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
          const tone = xruns > 0 || pct >= 100 ? "var(--status-alarm-2)" : pct >= 70 ? "var(--status-warn)" : "var(--status-ok)";
          const text = xruns > 0 ? `RT ⚠ ${xruns}` : `RT ${pct}%`;
          return (
            <span className="mono cpu" style={{ color: tone }} title={`Audio-thread load ${pct}%${xruns > 0 ? ` · ${xruns} overruns` : ""}`}>
              {text}
            </span>
          );
        })()}
        <button className="icon-btn" disabled title="Settings"><Icon name="settings" size={16} /></button>
        <span className="avatar">M</span>
      </div>
    </header>
  );
}
