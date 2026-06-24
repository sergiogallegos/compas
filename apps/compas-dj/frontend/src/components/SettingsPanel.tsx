import type { AuxApi } from "../hooks/useAux";
import type { BoothApi } from "../hooks/useBooth";
import type { CueApi } from "../hooks/useCue";
import { Icon } from "./icons";

export function SettingsPanel({
  aux,
  booth,
  cue,
  contrast,
  onToggleContrast,
  onOpenKeys,
  onOpenMap,
  onOpenPads,
  onOpenControllers,
  onClose,
}: {
  aux: AuxApi;
  booth: BoothApi;
  cue: CueApi;
  contrast: boolean;
  onToggleContrast: () => void;
  onOpenKeys: () => void;
  onOpenMap: () => void;
  onOpenPads: () => void;
  onOpenControllers: () => void;
  onClose: () => void;
}) {
  return (
    <div className="panel-overlay" onClick={onClose}>
      <div className="panel settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="panel-head">
          <h3>Settings</h3>
          <button className="chip" onClick={onClose}>Close</button>
        </div>
        <section className="settings-audio" aria-labelledby="settings-audio-heading">
          <div className="settings-section-head">
            <h4 id="settings-audio-heading">Audio devices</h4>
            <button
              className="chip settings-rescan"
              onClick={() => {
                aux.rescan();
                cue.rescan();
                booth.rescan();
              }}
            >
              Rescan
            </button>
          </div>
          <div className="settings-device-list">
            <DevicePicker
              icon="headphones"
              label="Headphones"
              value={cue.device}
              devices={cue.devices}
              defaultLabel="Default output"
              disabled={cue.enabled}
              status={cue.enabled ? `On: ${cue.connectedName ?? "selected output"}` : "Off"}
              onChange={cue.setDevice}
            />
            <DevicePicker
              icon="sliders"
              label="Booth"
              value={booth.device}
              devices={booth.devices}
              defaultLabel="Default output"
              disabled={booth.enabled}
              status={booth.enabled ? `On: ${booth.connectedName ?? "selected output"}` : "Off"}
              onChange={booth.setDevice}
            />
            <DevicePicker
              icon="mic"
              label="Aux input"
              value={aux.device}
              devices={aux.devices}
              defaultLabel="Default input"
              disabled={aux.enabled}
              status={aux.enabled ? `On: ${aux.connectedName ?? "selected input"}` : "Off"}
              onChange={aux.setDevice}
            />
          </div>
        </section>
        <div className="settings-grid">
          <button className={`settings-tile ${contrast ? "settings-tile--on" : ""}`} onClick={onToggleContrast}>
            <Icon name="sun" size={18} />
            <span>High contrast</span>
          </button>
          <button className="settings-tile" onClick={onOpenKeys}>
            <Icon name="music" size={18} />
            <span>Synth</span>
          </button>
          <button className="settings-tile" onClick={onOpenMap}>
            <Icon name="sliders" size={18} />
            <span>MIDI map</span>
          </button>
          <button className="settings-tile" onClick={onOpenPads}>
            <Icon name="pads" size={18} />
            <span>Sampler</span>
          </button>
          <button className="settings-tile" onClick={onOpenControllers}>
            <Icon name="sliders" size={18} />
            <span>Controllers</span>
          </button>
        </div>
      </div>
    </div>
  );
}

function DevicePicker({
  icon,
  label,
  value,
  devices,
  defaultLabel,
  disabled,
  status,
  onChange,
}: {
  icon: "headphones" | "mic" | "sliders";
  label: string;
  value: string | null;
  devices: string[];
  defaultLabel: string;
  disabled: boolean;
  status: string;
  onChange: (name: string | null) => void;
}) {
  return (
    <label className="settings-device-row">
      <span className="settings-device-title">
        <Icon name={icon} size={16} />
        <span>{label}</span>
      </span>
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value || null)}
        disabled={disabled}
      >
        <option value="">{defaultLabel}</option>
        {devices.map((device) => (
          <option key={device} value={device}>{device}</option>
        ))}
      </select>
      <span className="settings-device-status">{status}</span>
    </label>
  );
}
