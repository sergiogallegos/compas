/**
 * compas brand mark — "Needle & Rose": a beat-ticked compass ring with a needle
 * (= tonearm). Lifted from the design handoff (compas Logo.dc.html, primary mark).
 * The proportions here favor small sizes (title bar / icon).
 */
export function Logo({
  size = 22,
  glow = true,
  accent = "var(--accent)",
  south = "#3a3a45",
  ring = "rgba(255,255,255,.22)",
  hub = "var(--bg-window)",
}: {
  size?: number;
  glow?: boolean;
  accent?: string;
  south?: string;
  ring?: string;
  hub?: string;
}) {
  return (
    <svg
      viewBox="0 0 120 120"
      width={size}
      height={size}
      style={glow ? { filter: "drop-shadow(0 0 6px rgba(var(--accent-rgb),.5))" } : undefined}
      aria-label="compas"
    >
      <circle cx="60" cy="60" r="54" fill="none" stroke={ring} strokeWidth="6" />
      <g stroke={accent} strokeWidth="7" strokeLinecap="round">
        <line x1="60" y1="6" x2="60" y2="20" />
        <line x1="114" y1="60" x2="100" y2="60" />
        <line x1="60" y1="114" x2="60" y2="100" />
        <line x1="6" y1="60" x2="20" y2="60" />
      </g>
      <polygon points="60,20 72,62 60,71 48,62" fill={accent} />
      <polygon points="60,100 48,56 60,49 72,56" fill={south} />
      <circle cx="60" cy="60" r="9" fill={hub} stroke={accent} strokeWidth="5" />
    </svg>
  );
}
