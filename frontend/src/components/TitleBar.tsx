import { Logo } from "./Logo";
import { Icon } from "./icons";
import type { MasterMeter } from "../lib/ipc";

export function TitleBar({ masterBpm, master }: { masterBpm: number | null; master: MasterMeter }) {
  const bar = (v: number) => `${Math.min(100, Math.sqrt(Math.max(0, v)) * 100)}%`;
  return (
    <header className="titlebar">
      <div className="lights">
        <span style={{ background: "#ff5f57" }} />
        <span style={{ background: "#febc2e" }} />
        <span style={{ background: "#28c840" }} />
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
          <button className="sync-chip" disabled title="Sync engine: Phase 4">
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
