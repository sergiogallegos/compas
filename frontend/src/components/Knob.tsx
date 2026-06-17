import { useCallback, useRef, type PointerEvent } from "react";

/** Rotary knob: drag vertically to change. SVG body + pointer rotated to value.
 *  Sweep is 270° (from -135° to +135°). */
export function Knob({
  value,
  min,
  max,
  onChange,
  label,
  size = 36,
  color = "var(--text-secondary)",
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

  const norm = (value - min) / (max - min); // 0..1
  const angle = -135 + norm * 270;

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
        onDoubleClick={() => !disabled && onChange((min + max) / 2)}
        style={{ touchAction: "none", cursor: disabled ? "not-allowed" : "ns-resize" }}
      >
        <circle cx="20" cy="20" r="17" fill="var(--surface-control)" stroke="var(--border-hairline-strong)" />
        <g transform={`rotate(${angle} 20 20)`}>
          <line x1="20" y1="20" x2="20" y2="6" stroke={disabled ? "var(--text-disabled)" : color} strokeWidth="2.5" strokeLinecap="round" />
        </g>
      </svg>
      {label && <span className="knob-label overline">{label}</span>}
    </div>
  );
}
