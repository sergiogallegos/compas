import { useCallback, useId, useRef, type PointerEvent } from "react";

/** Rotary knob (Traktor/Serato-style): a 270° value arc around a beveled cap with a short
 *  indicator notch. Bipolar controls (EQ/filter — `min < 0 < max`) detent at 12 o'clock and fill
 *  the arc symmetrically from center; unipolar controls fill from the start. Drag vertically to
 *  change; double-click resets (to center if bipolar, else mid-range). */
const A0 = -135; // sweep start (deg; 0 = 12 o'clock straight up, + = clockwise)
const A1 = 135; // sweep end
const CX = 20;
const CY = 20;
const R = 16.5; // arc radius
const SW = 3.2; // arc stroke width

/** Cartesian point for a knob angle (deg; 0 = up, + = clockwise). */
function polar(r: number, deg: number): [number, number] {
  const a = (deg * Math.PI) / 180;
  return [CX + r * Math.sin(a), CY - r * Math.cos(a)];
}
/** SVG arc path from `from`→`to` (deg, clockwise). */
function arcPath(from: number, to: number): string {
  const [x0, y0] = polar(R, from);
  const [x1, y1] = polar(R, to);
  const large = Math.abs(to - from) > 180 ? 1 : 0;
  return `M ${x0.toFixed(2)} ${y0.toFixed(2)} A ${R} ${R} 0 ${large} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
}

export function Knob({
  value,
  min,
  max,
  onChange,
  label,
  size = 36,
  color = "var(--accent)",
  disabled = false,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  label?: string;
  size?: number;
  color?: string;
  disabled?: boolean;
}) {
  const drag = useRef<{ y: number; v: number } | null>(null);
  const capId = "kc" + useId().replace(/:/g, "");

  const bipolar = min < 0 && max > 0; // EQ / filter — detent at 12 o'clock
  const center = bipolar ? 0 : min;

  // Value → angle. Bipolar maps each side of center onto its own 135° half so the detent stays
  // straight up even when the range is asymmetric (EQ is −26..+6). Unipolar is a plain linear sweep.
  let angle: number;
  if (bipolar) {
    angle = value >= center ? ((value - center) / (max - center)) * A1 : ((value - center) / (center - min)) * A1;
  } else {
    angle = A0 + ((value - min) / (max - min)) * (A1 - A0);
  }
  angle = Math.max(A0, Math.min(A1, angle));

  // Filled portion of the arc: from center (bipolar) or start (unipolar) to the current angle.
  const fillFrom = bipolar ? Math.min(0, angle) : A0;
  const fillTo = bipolar ? Math.max(0, angle) : angle;
  const showFill = Math.abs(fillTo - fillFrom) > 0.5;
  const dim = disabled ? "var(--text-disabled)" : color;

  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      if (disabled) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = { y: e.clientY, v: value };
    },
    [disabled, value],
  );

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
      if (!drag.current) return;
      const dy = drag.current.y - e.clientY; // up = increase
      const range = max - min;
      const next = Math.min(max, Math.max(min, drag.current.v + (dy / 180) * range));
      onChange(next);
    },
    [min, max, onChange],
  );

  const end = useCallback(() => {
    drag.current = null;
  }, []);

  return (
    <div className={`knob ${disabled ? "knob--disabled" : ""}`}>
      <svg
        width={size}
        height={size}
        viewBox="0 0 40 40"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={end}
        onPointerLeave={end}
        onDoubleClick={() => !disabled && onChange(bipolar ? center : (min + max) / 2)}
        style={{ touchAction: "none", cursor: disabled ? "not-allowed" : "ns-resize" }}
      >
        <defs>
          <radialGradient id={capId} cx="50%" cy="36%" r="68%">
            <stop offset="0%" stopColor="#30303a" />
            <stop offset="100%" stopColor="#131318" />
          </radialGradient>
        </defs>
        {/* unfilled track */}
        <path d={arcPath(A0, A1)} fill="none" stroke="var(--surface-control)" strokeWidth={SW} strokeLinecap="round" />
        {/* filled value arc */}
        {showFill && !disabled && (
          <path d={arcPath(fillFrom, fillTo)} fill="none" stroke={color} strokeWidth={SW} strokeLinecap="round" />
        )}
        {/* beveled cap */}
        <circle cx={CX} cy={CY} r="12.5" fill={`url(#${capId})`} stroke="var(--border-hairline-strong)" />
        {/* indicator notch near the rim */}
        <g transform={`rotate(${angle} ${CX} ${CY})`}>
          <line x1={CX} y1="9.5" x2={CX} y2="15" stroke={dim} strokeWidth="2.4" strokeLinecap="round" />
        </g>
      </svg>
      {label && <span className="knob-label overline">{label}</span>}
    </div>
  );
}
