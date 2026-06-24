import { Icon } from "./icons";

export function SettingsPanel({
  contrast,
  onToggleContrast,
  onOpenKeys,
  onOpenMap,
  onOpenPads,
  onOpenControllers,
  onClose,
}: {
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
