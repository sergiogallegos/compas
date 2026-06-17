import { useCallback, useRef, type PointerEvent } from "react";

/** Custom fader (vertical or horizontal) driven by pointer drag — avoids the raw,
 *  inconsistent look/sizing of native vertical <input type=range>. */
export function Fader({
  value,
  min,
  max,
  onChange,
  orientation = "vertical",
  length = 120,
  fill = false,
  color = "var(--text-secondary)",
  center = false,
  disabled = false,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
  orientation?: "vertical" | "horizontal";
  length?: number;
  fill?: boolean;
  color?: string;
  center?: boolean;
  disabled?: boolean;
}) {
  const drag = useRef<{ pos: number; v: number } | null>(null);
  const vertical = orientation === "vertical";
  // clamp so a value driven beyond range (e.g. SYNC) doesn't push the handle off-track
  const norm = Math.min(1, Math.max(0, (value - min) / (max - min)));

  const onDown = useCallback(
    (e: PointerEvent) => {
      if (disabled) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = { pos: vertical ? e.clientY : e.clientX, v: value };
    },
    [disabled, value, vertical],
  );

  const onMove = useCallback(
    (e: PointerEvent) => {
      if (!drag.current) return;
      const cur = vertical ? e.clientY : e.clientX;
      // vertical: up increases; horizontal: right increases
      const delta = vertical ? drag.current.pos - cur : cur - drag.current.pos;
      const next = Math.min(max, Math.max(min, drag.current.v + (delta / length) * (max - min)));
      onChange(next);
    },
    [vertical, min, max, length, onChange],
  );

  const end = useCallback(() => {
    drag.current = null;
  }, []);

  const handleStyle = vertical
    ? { bottom: `calc(${norm * 100}% - 8px)`, borderColor: color }
    : { left: `calc(${norm * 100}% - 8px)`, borderColor: color };

  return (
    <div
      className={`fader ${vertical ? "fader--v" : "fader--h"} ${disabled ? "fader--disabled" : ""}`}
      style={vertical ? { height: fill ? "100%" : length } : { width: fill ? "100%" : length }}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={end}
      onPointerLeave={end}
      onDoubleClick={() => !disabled && center && onChange((min + max) / 2)}
    >
      <div className="fader-track" />
      {center && <div className="fader-detent" />}
      <div className="fader-handle" style={handleStyle} />
    </div>
  );
}
