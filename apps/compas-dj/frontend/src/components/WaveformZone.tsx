import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import type { DeckState } from "../hooks/useDeck";
import { bandColor } from "../lib/ipc";

const VW = 1368;
const VH = 80;
const NOW_FRAC = 0.38; // NOW playhead sits 38% from the left; track scrolls under it
const ZOOMS = [4, 8, 16, 32]; // seconds visible (preset buttons)
const MIN_VIEW = 2; // most zoomed-in (seconds visible)
const MAX_VIEW = 64; // most zoomed-out
const WHEEL_STEP = 0.85; // per wheel notch: <1 zooms in (fewer seconds visible)
const CLICK_STEP = 0.7; // per Ctrl/⌘+click step

interface Lane {
  state: DeckState;
  letter: string;
  color: string;
  onSeek: (frac: number) => void;
  onNudgeGrid?: (deltaSec: number) => void;
  onResetGrid?: () => void;
}

export function WaveformZone({ lanes }: { lanes: Lane[] }) {
  const [view, setView] = useState(8);
  const zoomBy = useCallback((factor: number) => {
    setView((v) => Math.min(MAX_VIEW, Math.max(MIN_VIEW, v * factor)));
  }, []);
  // Highlight the preset closest to the current (possibly wheel-adjusted) view.
  const nearest = ZOOMS.reduce((a, b) => (Math.abs(b - view) < Math.abs(a - view) ? b : a));
  return (
    <div className="wf-zone">
      <div className="wf-zoom">
        {ZOOMS.map((z) => (
          <button key={z} className={`wf-zoom-btn ${z === nearest ? "wf-zoom-btn--on" : ""}`} onClick={() => setView(z)}>
            {z}s
          </button>
        ))}
      </div>
      {/* single NOW playhead spanning both lanes */}
      <div className="wf-now" />
      <div className="wf-now-tag mono">NOW</div>
      {lanes.map((lane, i) => (
        <WaveLane key={i} lane={lane} view={view} zoomBy={zoomBy} />
      ))}
    </div>
  );
}

function WaveLane({ lane, view, zoomBy }: { lane: Lane; view: number; zoomBy: (factor: number) => void }) {
  const { state, color, letter, onSeek, onNudgeGrid, onResetGrid } = lane;
  const meta = state.meta;
  const streaming = !state.dsp;
  const effBpm = meta ? meta.bpm * state.tempo : 0;

  const sr = meta?.source_rate ?? 1;
  const frames = meta?.frames ?? 0;
  const duration = frames > 0 ? frames / sr : 0;
  const peaks = meta?.peaks ?? [];
  const binSec = peaks.length > 0 && duration > 0 ? duration / peaks.length : 0;
  // Frequency-band energy per bin (low→R, mid→G, high→B), aligned 1:1 with `peaks`.
  const bands = meta?.band_peaks ?? [];
  const useBands = bands.length === peaks.length && bands.length > 0;

  // Play-head position (event-driven at the telemetry rate), offset by DAC latency so the marker
  // matches what's being heard. (A display-rate rAF here re-rendered the whole band waveform every
  // frame and saturated the UI; keep it event-driven.)
  const nowTime = Math.max(0, state.frame - state.rate * state.latencySecs) / sr;
  const t0 = nowTime - NOW_FRAC * view; // left edge time
  const t1 = t0 + view;
  // Overscan the right edge so the rAF interpolation (below) has drawn content to scroll in
  // between the 30 Hz position samples — otherwise a thin gap would flicker at the right.
  const overSec = view * 0.08;
  const t1o = t1 + overSec;

  const xOf = (t: number) => ((t - t0) / view) * VW;

  // Smooth scroll: position events arrive at the telemetry rate (30 Hz), which alone looks
  // stepped. We translate the rendered <g> by the sub-sample time elapsed since the last event
  // via requestAnimationFrame — a GPU-composited transform, so the waveform/beat paths are NOT
  // re-rendered each frame (the cost the event-driven design deliberately avoids).
  const gRef = useRef<SVGGElement>(null);
  const animRef = useRef({ playing: false, rate: 0, sr: 1, view, frameAt: 0 });
  animRef.current = { playing: state.playing, rate: state.rate, sr, view, frameAt: state.frameAt };
  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const g = gRef.current;
      if (g) {
        const a = animRef.current;
        let dxFrac = 0;
        if (a.playing && a.frameAt > 0 && a.sr > 0 && a.view > 0) {
          const elapsed = (performance.now() - a.frameAt) / 1000; // s since last sample
          dxFrac = (elapsed * a.rate) / (a.sr * a.view); // fraction of the window advanced
          dxFrac = Math.max(0, Math.min(0.08, dxFrac)); // cap at the overscan margin
        }
        g.setAttribute("transform", `translate(${(-dxFrac * VW).toFixed(2)} 0)`);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  // waveform slice
  const slice = (scale: number): string => {
    if (binSec <= 0) return "";
    const cy = VH / 2;
    const i0 = Math.max(0, Math.floor(t0 / binSec));
    const i1 = Math.min(peaks.length - 1, Math.ceil(t1o / binSec));
    if (i1 <= i0) return "";
    let d = `M ${xOf(i0 * binSec).toFixed(1)} ${cy}`;
    for (let i = i0; i <= i1; i++) {
      d += ` L ${xOf(i * binSec).toFixed(1)} ${(cy - Math.min(1, peaks[i]) * cy * scale).toFixed(1)}`;
    }
    for (let i = i1; i >= i0; i--) {
      d += ` L ${xOf(i * binSec).toFixed(1)} ${(cy + Math.min(1, peaks[i]) * cy * scale).toFixed(1)}`;
    }
    return d + " Z";
  };

  // beat lines in the visible window
  const beats: { x: number; down: boolean }[] = [];
  const interval = meta?.beat_interval_sec ?? 0;
  const offset = (meta?.first_beat_sec ?? 0) + state.gridOffset;
  if (interval > 0.05) {
    let k = Math.floor((t0 - offset) / interval) - 1;
    for (let n = 0; n < 4096; n++, k++) {
      const t = offset + k * interval;
      if (t > t1o) break;
      if (t >= t0 && t >= 0) beats.push({ x: xOf(t), down: ((k % 4) + 4) % 4 === 0 });
    }
  }

  // Grab-scrub the lane like vinyl: pointer-down anchors at the current play position, and dragging
  // moves the track under the fixed NOW needle (drag right → rewind). Using a frozen anchor (startSec
  // captured at press) avoids the feedback loop an absolute clientX→time mapping would cause as the
  // seek shifts t0 under the cursor. A press with no real movement falls back to a needle-drop.
  const drag = useRef<{ startX: number; startSec: number; moved: boolean } | null>(null);
  const needleDrop = (clientX: number, r: DOMRect) => {
    const frac = (clientX - r.left) / r.width;
    const target = t0 + frac * view;
    onSeek(Math.min(1, Math.max(0, target / duration)));
  };

  // Wheel zoom: React binds `onWheel` passively, so a native non-passive listener is needed to
  // preventDefault (otherwise the gesture would also scroll the page). Scroll up zooms in.
  const laneRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = laneRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      zoomBy(e.deltaY < 0 ? WHEEL_STEP : 1 / WHEEL_STEP);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomBy]);

  const onDown = (e: PointerEvent<HTMLDivElement>) => {
    if (duration <= 0) return;
    // Ctrl/⌘+click steps zoom (hold Shift to zoom out) instead of scrubbing.
    if (e.ctrlKey || e.metaKey) {
      zoomBy(e.shiftKey ? 1 / CLICK_STEP : CLICK_STEP);
      return;
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { startX: e.clientX, startSec: nowTime, moved: false };
  };
  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || duration <= 0) return;
    const r = e.currentTarget.getBoundingClientRect();
    const dx = e.clientX - d.startX;
    if (Math.abs(dx) > 3) d.moved = true;
    if (!d.moved) return;
    const target = d.startSec - (dx / r.width) * view; // grab: drag right rewinds, left fast-forwards
    onSeek(Math.min(1, Math.max(0, target / duration)));
  };
  const onUp = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    drag.current = null;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      // capture may already be lost (e.g. pointercancel) — nothing to release
    }
    if (d && !d.moved && duration > 0) needleDrop(e.clientX, e.currentTarget.getBoundingClientRect());
  };

  return (
    <div className="wf-lane" ref={laneRef} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
      {peaks.length > 0 && (
        <svg className="wf-svg" viewBox={`0 0 ${VW} ${VH}`} preserveAspectRatio="none">
          <g ref={gRef}>
          <g>
            {beats.map((b, i) => (
              <line key={i} x1={b.x} y1={0} x2={b.x} y2={VH} stroke={b.down ? color : "rgba(255,255,255,.12)"} strokeWidth={b.down ? 1.6 : 0.7} opacity={b.down ? 0.6 : 1} />
            ))}
          </g>
          {useBands ? (
            <g>
              {(() => {
                const cy = VH / 2;
                const i0 = Math.max(0, Math.floor(t0 / binSec));
                const i1 = Math.min(peaks.length - 1, Math.ceil(t1o / binSec));
                // Cap at ~one line per pixel column so wide zooms stay cheap to render.
                const step = Math.max(1, Math.ceil((i1 - i0) / VW));
                const bw = Math.max(0.7, (binSec / view) * VW * step);
                const bars = [];
                for (let i = i0; i <= i1; i += step) {
                  const x = xOf(i * binSec);
                  const amp = Math.min(1, peaks[i]) * cy * 0.92;
                  bars.push(
                    <line key={i} x1={x} y1={cy - amp} x2={x} y2={cy + amp} stroke={bandColor(bands[i])} strokeWidth={bw} />,
                  );
                }
                return bars;
              })()}
            </g>
          ) : (
            <>
              <path d={slice(1)} fill={color} opacity={0.3} />
              <path d={slice(0.46)} fill={color} opacity={0.92} />
            </>
          )}
          </g>
        </svg>
      )}
      <div className="wf-scrim" style={{ width: `${NOW_FRAC * 100}%` }} />
      {state.loop.active &&
        (() => {
          const left = Math.max(0, (state.loop.inFrame / sr - t0) / view);
          const right = Math.min(1, (state.loop.outFrame / sr - t0) / view);
          if (right <= 0 || left >= 1 || right <= left) return null;
          return (
            <div
              className="wf-loop"
              style={{ left: `${left * 100}%`, width: `${(right - left) * 100}%`, background: `${color}22`, borderColor: color }}
            />
          );
        })()}
      {!state.loop.active &&
        state.loop.armed &&
        (() => {
          const x = (state.loop.inFrame / sr - t0) / view;
          if (x < 0 || x > 1) return null;
          return <div className="wf-loop-in" style={{ left: `${x * 100}%`, background: color, boxShadow: `0 0 6px ${color}` }} />;
        })()}
      {streaming && <div className="wf-hatch" />}
      <div className="wf-label">
        <span className="wf-chip" style={{ color, background: `${color}1f`, borderColor: `${color}66` }}>{letter}</span>
        <span className="mono wf-bpm">
          {meta && effBpm > 0 ? `${effBpm.toFixed(1)} · ${meta.key_camelot}` : "— · —"}
        </span>
        <span className="wf-title">{meta ? `${meta.title} — ${meta.artist}` : "No track loaded"}</span>
        {streaming && <span className="wf-badge">STREAM · CONTROL-ONLY</span>}
        {meta && interval > 0.05 && onNudgeGrid && (
          <span className="wf-grid-edit" onPointerDown={(e) => e.stopPropagation()} title="Nudge the beatgrid to line it up with the audio">
            <button onClick={() => onNudgeGrid(-0.005)}>◀</button>
            <button onClick={onResetGrid} className="wf-grid-reset">
              GRID{Math.abs(state.gridOffset) > 1e-4 ? ` ${state.gridOffset > 0 ? "+" : ""}${(state.gridOffset * 1000).toFixed(0)}ms` : ""}
            </button>
            <button onClick={() => onNudgeGrid(0.005)}>▶</button>
          </span>
        )}
      </div>
    </div>
  );
}
