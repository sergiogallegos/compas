import { useState, type PointerEvent } from "react";
import type { DeckState } from "../hooks/useDeck";
import { bandColor } from "../lib/ipc";

const VW = 1368;
const VH = 80;
const NOW_FRAC = 0.38; // NOW playhead sits 38% from the left; track scrolls under it
const ZOOMS = [4, 8, 16, 32]; // seconds visible

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
  return (
    <div className="wf-zone">
      <div className="wf-zoom">
        {ZOOMS.map((z) => (
          <button key={z} className={`wf-zoom-btn ${z === view ? "wf-zoom-btn--on" : ""}`} onClick={() => setView(z)}>
            {z}s
          </button>
        ))}
      </div>
      {/* single NOW playhead spanning both lanes */}
      <div className="wf-now" />
      <div className="wf-now-tag mono">NOW</div>
      {lanes.map((lane, i) => (
        <WaveLane key={i} lane={lane} view={view} />
      ))}
    </div>
  );
}

function WaveLane({ lane, view }: { lane: Lane; view: number }) {
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

  const xOf = (t: number) => ((t - t0) / view) * VW;

  // waveform slice
  const slice = (scale: number): string => {
    if (binSec <= 0) return "";
    const cy = VH / 2;
    const i0 = Math.max(0, Math.floor(t0 / binSec));
    const i1 = Math.min(peaks.length - 1, Math.ceil(t1 / binSec));
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
      if (t > t1) break;
      if (t >= t0 && t >= 0) beats.push({ x: xOf(t), down: ((k % 4) + 4) % 4 === 0 });
    }
  }

  const handle = (e: PointerEvent<HTMLDivElement>) => {
    if (duration <= 0) return;
    const r = e.currentTarget.getBoundingClientRect();
    const frac = (e.clientX - r.left) / r.width;
    const target = t0 + frac * view; // seek within the visible window
    onSeek(Math.min(1, Math.max(0, target / duration)));
  };

  return (
    <div className="wf-lane" onPointerDown={handle}>
      {peaks.length > 0 && (
        <svg className="wf-svg" viewBox={`0 0 ${VW} ${VH}`} preserveAspectRatio="none">
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
                const i1 = Math.min(peaks.length - 1, Math.ceil(t1 / binSec));
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
