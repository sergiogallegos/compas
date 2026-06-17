import type { PointerEvent } from "react";
import type { DeckState } from "../hooks/useDeck";

const COLS = 640;
const VW = 1368;
const VH = 80;

function buildPath(peaks: number[], scale: number): string {
  if (peaks.length === 0) return "";
  const cy = VH / 2;
  const amp = new Array<number>(COLS).fill(0);
  for (let c = 0; c < COLS; c++) {
    const start = Math.floor((c / COLS) * peaks.length);
    const end = Math.max(start + 1, Math.floor(((c + 1) / COLS) * peaks.length));
    let m = 0;
    for (let i = start; i < end && i < peaks.length; i++) m = Math.max(m, peaks[i]);
    amp[c] = Math.min(1, m);
  }
  const dx = VW / (COLS - 1);
  let d = `M 0 ${cy} `;
  for (let i = 0; i < COLS; i++) d += `L ${(i * dx).toFixed(1)} ${(cy - amp[i] * cy * scale).toFixed(1)} `;
  for (let i = COLS - 1; i >= 0; i--) d += `L ${(i * dx).toFixed(1)} ${(cy + amp[i] * cy * scale).toFixed(1)} `;
  return d + "Z";
}

interface Lane {
  state: DeckState;
  letter: string;
  color: string;
  onSeek: (frac: number) => void;
}

export function WaveformZone({ lanes }: { lanes: Lane[] }) {
  return (
    <div className="wf-zone">
      {lanes.map((lane, i) => (
        <WaveLane key={i} lane={lane} />
      ))}
    </div>
  );
}

function WaveLane({ lane }: { lane: Lane }) {
  const { state, letter, color, onSeek } = lane;
  const frames = state.meta?.frames ?? 0;
  const frac = frames > 0 ? Math.min(1, state.frame / frames) : 0;
  const streaming = !state.dsp;
  const peaks = state.meta?.peaks ?? [];
  const effBpm = state.meta ? state.meta.bpm * state.tempo : 0;

  // Beatgrid: vertical lines at first_beat + k*interval, mapped to the overview width.
  const durationSec = state.meta && state.meta.source_rate > 0 ? state.meta.frames / state.meta.source_rate : 0;
  const interval = state.meta?.beat_interval_sec ?? 0;
  const offset = state.meta?.first_beat_sec ?? 0;
  const beats: { x: number; down: boolean }[] = [];
  if (durationSec > 0 && interval > 0.05) {
    for (let k = 0, t = offset; t <= durationSec && beats.length < 4096; k++, t = offset + k * interval) {
      beats.push({ x: (t / durationSec) * VW, down: k % 4 === 0 });
    }
  }

  const handle = (e: PointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    onSeek(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)));
  };

  return (
    <div className="wf-lane" onPointerDown={handle}>
      {peaks.length > 0 && (
        <svg className="wf-svg" viewBox={`0 0 ${VW} ${VH}`} preserveAspectRatio="none">
          <g>
            {beats.map((b, i) => (
              <line
                key={i}
                x1={b.x}
                y1={0}
                x2={b.x}
                y2={VH}
                stroke={b.down ? color : "rgba(255,255,255,.12)"}
                strokeWidth={b.down ? 1.6 : 0.7}
                opacity={b.down ? 0.55 : 1}
              />
            ))}
          </g>
          <path d={buildPath(peaks, 1)} fill={color} opacity={0.3} />
          <path d={buildPath(peaks, 0.46)} fill={color} opacity={0.92} />
        </svg>
      )}
      <div className="wf-scrim" style={{ width: `${frac * 100}%` }} />
      {streaming && <div className="wf-hatch" />}
      {frames > 0 && (
        <div className="wf-playhead" style={{ left: `${frac * 100}%`, background: color, boxShadow: `0 0 10px ${color}` }} />
      )}
      <div className="wf-label">
        <span className="wf-chip" style={{ color, background: `${color}1f`, borderColor: `${color}66` }}>
          {letter}
        </span>
        <span className="mono wf-bpm">{state.meta && effBpm > 0 ? `${effBpm.toFixed(1)} · —` : "— · —"}</span>
        <span className="wf-title">
          {state.meta ? `${state.meta.title} — ${state.meta.artist}` : "No track loaded"}
        </span>
        {streaming && <span className="wf-badge">STREAM · CONTROL-ONLY</span>}
      </div>
    </div>
  );
}
