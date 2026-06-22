import { Icon } from "./icons";

export type NavTarget = "perform" | "library" | "crates" | "fx" | "rec";

const ITEMS = [
  { id: "perform" as const, name: "Perform", icon: "perform" as const },
  { id: "library" as const, name: "Library", icon: "music" as const },
  { id: "crates" as const, name: "Crates", icon: "list" as const },
  { id: "fx" as const, name: "FX", icon: "sliders" as const },
  { id: "rec" as const, name: "Rec", icon: "mic" as const },
];

export function NavRail({
  active,
  onSelect,
  contrast,
  onToggleContrast,
}: {
  active: NavTarget;
  onSelect: (target: NavTarget) => void;
  contrast: boolean;
  onToggleContrast: () => void;
}) {
  return (
    <nav className="navrail">
      <div className="nav-items">
        {ITEMS.map((it) => (
          <button
            key={it.name}
            className={`nav-item ${active === it.id ? "nav-item--active" : ""}`}
            onClick={() => onSelect(it.id)}
            title={it.name}
          >
            {active === it.id && <span className="nav-bar" />}
            <Icon name={it.icon} size={22} />
            <span className="nav-label">{it.name}</span>
          </button>
        ))}
      </div>
      <button
        className={`nav-item nav-settings ${contrast ? "nav-item--active" : ""}`}
        onClick={onToggleContrast}
        title={contrast ? "Standard contrast" : "High contrast"}
      >
        {contrast && <span className="nav-bar" />}
        <Icon name="sun" size={20} />
      </button>
    </nav>
  );
}
