import { useState, type PointerEvent } from "react";
import type { DeckState } from "../hooks/useDeck";

const VW = 1368;
const VH = 80;
const NOW_FRAC = 0.38; // NOW playhead sits 38% from the left; track scrolls under it
const ZOOMS = [4, 8, 16, 32]; // seconds visible

interface Lane {
  state: DeckState;
  letter: string;
  color: string;
  onSeek: (frac: number) => void;
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
  const { state, color, letter, onSeek } = lane;
  const meta = state.meta;
  const streaming = !state.dsp;
  const effBpm = meta ? meta.bpm * state.tempo : 0;

  const sr = meta?.source_rate ?? 1;
  const frames = meta?.frames ?? 0;
  const duration = frames > 0 ? frames / sr : 0;
  const peaks = meta?.peaks ?? [];
  const binSec = peaks.length > 0 && duration > 0 ? duration / peaks.length : 0;

  // Play-head already advances by tempo, so frame→time gives correct scroll speed.
  const nowTime = state.frame / sr; // seconds (source time)
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
  const offset = meta?.first_beat_sec ?? 0;
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
          <path d={slice(1)} fill={color} opacity={0.3} />
          <path d={slice(0.46)} fill={color} opacity={0.92} />
        </svg>
      )}
      <div className="wf-scrim" style={{ width: `${NOW_FRAC * 100}%` }} />
      {streaming && <div className="wf-hatch" />}
      <div className="wf-label">
        <span className="wf-chip" style={{ color, background: `${color}1f`, borderColor: `${color}66` }}>{letter}</span>
        <span className="mono wf-bpm">
          {meta && effBpm > 0 ? `${effBpm.toFixed(1)} · ${meta.key_camelot}` : "— · —"}
        </span>
        <span className="wf-title">{meta ? `${meta.title} — ${meta.artist}` : "No track loaded"}</span>
        {streaming && <span className="wf-badge">STREAM · CONTROL-ONLY</span>}
      </div>
    </div>
  );
}
