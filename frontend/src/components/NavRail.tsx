import { Icon } from "./icons";

const ITEMS = [
  { name: "Perform", icon: "perform" as const, active: true },
  { name: "Library", icon: "music" as const, active: false },
  { name: "Crates", icon: "list" as const, active: false },
  { name: "FX", icon: "sliders" as const, active: false },
  { name: "Rec", icon: "mic" as const, active: false },
];

export function NavRail() {
  return (
    <nav className="navrail">
      <div className="nav-items">
        {ITEMS.map((it) => (
          <button key={it.name} className={`nav-item ${it.active ? "nav-item--active" : ""}`} disabled={!it.active}>
            {it.active && <span className="nav-bar" />}
            <Icon name={it.icon} size={22} />
            <span className="nav-label">{it.name}</span>
          </button>
        ))}
      </div>
      <button className="nav-item nav-settings" disabled>
        <Icon name="sun" size={20} />
      </button>
    </nav>
  );
}
