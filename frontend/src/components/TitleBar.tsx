import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "./Logo";
import { Icon } from "./icons";
import { inTauri, type MasterMeter } from "../lib/ipc";

export function TitleBar({
  masterBpm,
  master,
  syncEnabled = false,
  onSync,
}: {
  masterBpm: number | null;
  master: MasterMeter;
  syncEnabled?: boolean;
  onSync?: () => void;
}) {
  const bar = (v: number) => `${Math.min(100, Math.sqrt(Math.max(0, v)) * 100)}%`;
  const win = () => (inTauri() ? getCurrentWindow() : null);
  return (
    <header className="titlebar" data-tauri-drag-region>
      {/* macOS-style controls: close / minimize / zoom */}
      <div className="lights">
        <button className="light" style={{ background: "#ff5f57" }} title="Close" onClick={() => win()?.close()} />
        <button className="light" style={{ background: "#febc2e" }} title="Minimize" onClick={() => win()?.minimize()} />
        <button className="light" style={{ background: "#28c840" }} title="Maximize" onClick={() => win()?.toggleMaximize()} />
      </div>
      <div className="brand">
        <Logo size={22} />
        <span className="brand-word display">compas</span>
      </div>

      <div className="master-cluster">
        <div className="master-pill">
          <div className="master-bpm">
            <span className="overline">MASTER</span>
            <span className="mono master-bpm-val">
              {masterBpm ? masterBpm.toFixed(1) : "—"}
              <small>BPM</small>
            </span>
          </div>
          <button
            className="sync-chip"
            onClick={onSync}
            disabled={!syncEnabled}
            title="Match Deck B's tempo to Deck A"
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
          <button className="mt-btn mt-rec" disabled title="Recording: Phase 5"><span className="rec-dot" /></button>
        </div>
      </div>

      <div className="titlebar-right">
        <span className="mono cpu">RT OK</span>
        <button className="icon-btn" disabled title="Settings"><Icon name="settings" size={16} /></button>
        <span className="avatar">M</span>
      </div>
    </header>
  );
}
