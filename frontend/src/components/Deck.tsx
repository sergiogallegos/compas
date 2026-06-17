import { useCallback, useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  deckPause,
  deckPlay,
  deckSeek,
  deckUnload,
  inTauri,
  onDeckError,
  onDeckLoaded,
  onDeckPosition,
  pickAudioFile,
  loadTrack,
  setDeckEq,
  setDeckFilter,
  setDeckGain,
  setDeckTempo,
  type DeckLoaded,
  type FilterMode,
} from "../lib/ipc";
import { Waveform } from "./Waveform";

const TEMPO_MIN = 0.92;
const TEMPO_MAX = 1.08;
const NUDGE = 1.03;

function formatTime(frame: number, rate: number): string {
  if (rate <= 0) return "0:00";
  const secs = Math.max(0, frame / rate);
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** Map a bipolar filter knob (-1..1) to engine params: center = off, left = LPF
 *  sweeping down, right = HPF sweeping up. */
function filterParams(x: number): { mode: FilterMode; cutoff: number; resonance: number } {
  if (Math.abs(x) < 0.02) return { mode: "off", cutoff: 1000, resonance: 0.9 };
  if (x < 0) return { mode: "lowpass", cutoff: 20000 * Math.pow(200 / 20000, -x), resonance: 0.9 + -x };
  return { mode: "highpass", cutoff: 20 * Math.pow(4000 / 20, x), resonance: 0.9 + x };
}

export function Deck({ deck, side }: { deck: number; side: "A" | "B" }) {
  const [meta, setMeta] = useState<DeckLoaded | null>(null);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [tempo, setTempo] = useState(1);
  const [eq, setEq] = useState({ low: 0, mid: 0, high: 0 });
  const [filter, setFilter] = useState(0);
  const [gain, setGain] = useState(1);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri()) return;
    const unlistens: UnlistenFn[] = [];
    let active = true;
    const track = (p: Promise<UnlistenFn>) =>
      p.then((u) => (active ? unlistens.push(u) : u()));

    track(
      onDeckLoaded((e) => {
        if (e.deck !== deck) return;
        setMeta(e);
        setFrame(0);
        setPlaying(false);
        setTempo(1);
        setError(null);
      }),
    );
    track(
      onDeckPosition((e) => {
        if (e.deck !== deck) return;
        setFrame(e.frame);
        setPlaying(e.playing);
      }),
    );
    track(
      onDeckError((e) => {
        if (e.deck === deck) setError(e.message);
      }),
    );

    return () => {
      active = false;
      unlistens.forEach((u) => u());
    };
  }, [deck]);

  const handleLoad = useCallback(async () => {
    try {
      const path = await pickAudioFile();
      if (path) await loadTrack(deck, path);
    } catch (e) {
      setError(String(e));
    }
  }, [deck]);

  const applyTempo = useCallback(
    (ratio: number) => {
      setTempo(ratio);
      setDeckTempo(deck, ratio).catch(() => {});
    },
    [deck],
  );

  const applyEq = useCallback(
    (next: { low: number; mid: number; high: number }) => {
      setEq(next);
      setDeckEq(deck, next.low, next.mid, next.high).catch(() => {});
    },
    [deck],
  );

  const applyFilter = useCallback(
    (x: number) => {
      setFilter(x);
      const p = filterParams(x);
      setDeckFilter(deck, p.mode, p.cutoff, p.resonance).catch(() => {});
    },
    [deck],
  );

  const frames = meta?.frames ?? 0;
  const rate = meta?.source_rate ?? 1;
  const positionFrac = frames > 0 ? frame / frames : 0;
  const effectiveBpm = meta ? meta.bpm * tempo : 0;

  return (
    <section className="deck">
      <header className="deck-head">
        <span className="deck-tag">Deck {side}</span>
        <button className="btn" onClick={handleLoad}>
          Load…
        </button>
        {meta && (
          <button
            className="btn"
            onClick={() => {
              deckUnload(deck).catch(() => {});
              setMeta(null);
              setPlaying(false);
            }}
          >
            Eject
          </button>
        )}
      </header>

      <div className="deck-meta">
        {meta ? (
          <>
            <div className="title">{meta.title}</div>
            <div className="artist">{meta.artist}</div>
          </>
        ) : (
          <div className="artist">{error ?? "No track loaded"}</div>
        )}
      </div>

      <Waveform
        peaks={meta?.peaks ?? []}
        positionFrac={positionFrac}
        onSeek={(f) => deckSeek(deck, f * frames).catch(() => {})}
      />

      <div className="deck-readout">
        <span>{formatTime(frame, rate)}</span>
        <span>/ {meta ? formatTime(frames, rate) : "0:00"}</span>
        <span className="bpm">
          {meta && meta.bpm > 0 ? (
            <>
              {effectiveBpm.toFixed(1)} BPM
              <small>
                {" "}
                (det. {meta.bpm.toFixed(1)}, conf {(meta.bpm_confidence * 100).toFixed(0)}%)
              </small>
            </>
          ) : (
            "— BPM"
          )}
        </span>
      </div>

      <div className="transport">
        <button className="btn" onClick={() => deckSeek(deck, 0).catch(() => {})}>
          ⟲ cue
        </button>
        <button
          className="btn play"
          onClick={() =>
            (playing ? deckPause(deck) : deckPlay(deck)).catch(() => {})
          }
          disabled={!meta}
        >
          {playing ? "⏸ pause" : "▶ play"}
        </button>
      </div>

      <div className="deck-grid">
        <label className="ctrl">
          Tempo {((tempo - 1) * 100).toFixed(1)}%
          <input
            type="range"
            min={TEMPO_MIN}
            max={TEMPO_MAX}
            step={0.0005}
            value={tempo}
            onChange={(e) => applyTempo(Number(e.target.value))}
          />
          <span className="nudge-row">
            <button
              className="btn small"
              onPointerDown={() => setDeckTempo(deck, tempo / NUDGE).catch(() => {})}
              onPointerUp={() => setDeckTempo(deck, tempo).catch(() => {})}
              onPointerLeave={() => setDeckTempo(deck, tempo).catch(() => {})}
            >
              −
            </button>
            <button
              className="btn small"
              onPointerDown={() => setDeckTempo(deck, tempo * NUDGE).catch(() => {})}
              onPointerUp={() => setDeckTempo(deck, tempo).catch(() => {})}
              onPointerLeave={() => setDeckTempo(deck, tempo).catch(() => {})}
            >
              +
            </button>
          </span>
        </label>

        <div className="eq">
          {(["high", "mid", "low"] as const).map((band) => (
            <label key={band} className="ctrl">
              {band.toUpperCase()}
              <input
                type="range"
                min={-26}
                max={6}
                step={0.5}
                value={eq[band]}
                onChange={(e) => applyEq({ ...eq, [band]: Number(e.target.value) })}
              />
            </label>
          ))}
        </div>

        <label className="ctrl">
          Filter {filter < -0.02 ? "LPF" : filter > 0.02 ? "HPF" : "off"}
          <input
            type="range"
            min={-1}
            max={1}
            step={0.01}
            value={filter}
            onChange={(e) => applyFilter(Number(e.target.value))}
          />
        </label>

        <label className="ctrl">
          Gain
          <input
            type="range"
            min={0}
            max={1.5}
            step={0.01}
            value={gain}
            onChange={(e) => {
              const v = Number(e.target.value);
              setGain(v);
              setDeckGain(deck, v).catch(() => {});
            }}
          />
        </label>
      </div>
    </section>
  );
}
