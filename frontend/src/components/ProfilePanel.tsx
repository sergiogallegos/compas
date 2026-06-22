export function ProfilePanel({
  name,
  onName,
  onClose,
}: {
  name: string;
  onName: (name: string) => void;
  onClose: () => void;
}) {
  const initial = name.trim().charAt(0).toUpperCase() || "M";
  return (
    <div className="panel-overlay" onClick={onClose}>
      <div className="panel profile-panel" onClick={(e) => e.stopPropagation()}>
        <div className="panel-head">
          <h3>Profile</h3>
          <button className="chip" onClick={onClose}>Close</button>
        </div>
        <div className="profile-row">
          <span className="avatar avatar--large">{initial}</span>
          <label className="profile-field">
            <span className="overline">DISPLAY NAME</span>
            <input value={name} onChange={(e) => onName(e.target.value)} maxLength={32} />
          </label>
        </div>
        <div className="profile-stats">
          <span className="mono">LOCAL</span>
          <span className="mono">MIT</span>
          <span className="mono">BETA</span>
        </div>
      </div>
    </div>
  );
}
