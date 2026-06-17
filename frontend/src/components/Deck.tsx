import type { DeckController } from "../hooks/useDeck";
import { Fader } from "./Fader";
import { Icon } from "./icons";

const CUE_COLORS = ["var(--accent)", "var(--stream)", "var(--status-warn)", "var(--status-ok)"];

function fmt(frame: number, rate: number): string {
  if (rate <= 0) return "0:00";
  const s = Math.max(0, frame / rate);
  return `${Math.floor(s / 60)}:${Math.floor(s % 60).toString().padStart(2, "0")}`;
}

export function Deck({
  ctrl,
  color,
  onSync,
  syncEnabled = false,
}: {
  ctrl: DeckController;
  color: string;
  onSync?: () => void;
  syncEnabled?: boolean;
}) {
  const { state, actions } = ctrl;
  const { meta, frame, playing, tempo, dsp, loading } = state;
  const rate = meta?.source_rate ?? 1;
  const frames = meta?.frames ?? 0;
  const frac = frames > 0 ? Math.min(1, frame / frames) : 0;
  const effBpm = meta ? meta.bpm * tempo : 0;
  const spin = (((frame / rate) * 200) % 360 + 360) % 360;

  // tempo fader works in percent for display; ratio for the engine.
  const pct = (tempo - 1) * 100;

  return (
    <section className="deck" style={{ borderTopColor: color }}>
      {/* header */}
      <div className="deck-header">
        <button
          className="art"
          style={{ background: `linear-gradient(135deg, ${color}, var(--violet))` }}
          onClick={actions.load}
          title={meta ? "Load a different track" : "Load a track"}
        >
          <Icon name="music" size={20} />
        </button>
        <div className="deck-titles">
          <div className="deck-title display">{loading ? "Loading…" : meta ? meta.title : "—"}</div>
          <div className="deck-sub">
            {loading ? "decoding & analyzing…" : meta ? meta.artist : "No track"}
          </div>
        </div>
        <div className="deck-header-actions">
          {loading ? (
            <button className="deck-load" disabled>Loading…</button>
          ) : meta ? (
            <button className="deck-load" onClick={actions.eject}>Eject</button>
          ) : (
            <button className="deck-load deck-load--primary" onClick={actions.load} style={{ borderColor: `${color}66`, color }}>
              Load…
            </button>
          )}
          <span className="src-badge" style={dsp ? { color: "var(--status-ok)", borderColor: "#3ddc9755", background: "#3ddc971a" } : { color: "var(--stream)", borderColor: "#28e0ff55", background: "#28e0ff1a" }}>
            {dsp ? "LOCAL · DSP" : "CONTROL-ONLY"}
          </span>
        </div>
      </div>

      {/* readout tiles */}
      <div className="readouts">
        <div className="tile">
          <span className="overline">BPM</span>
          <span className="mono tile-val">{meta && effBpm > 0 ? effBpm.toFixed(1) : "—"}</span>
        </div>
        <div className="tile">
          <span className="overline">KEY</span>
          <span className="mono tile-val" style={{ color }} title={meta?.key_name}>
            {meta?.key_camelot ?? "—"}
          </span>
        </div>
        <div className="tile">
          <span className="overline">TIME</span>
          <span className="mono tile-val small">
            {fmt(frame, rate)} <span className="muted">/ -{fmt(frames - frame, rate)}</span>
          </span>
        </div>
      </div>

      <div className="deck-body">
        {/* platter */}
        <div className="platter-col">
          <div className="platter">
            <div
              className="platter-ring"
              style={{ background: `conic-gradient(${color} ${frac * 360}deg, rgba(255,255,255,.07) 0)` }}
            />
            <div className="platter-marker" style={{ transform: `rotate(${spin}deg)`, background: color, boxShadow: `0 0 8px ${color}` }} />
            <div className="platter-center">
              <span className="mono platter-bpm">{meta && effBpm > 0 ? effBpm.toFixed(1) : "—"}</span>
              <span className="overline" style={{ color: playing ? color : "var(--text-tertiary)" }}>
                {playing ? "PLAYING" : "CUED"}
              </span>
            </div>
          </div>
          <div className="platter-btns">
            <button className="btn-cue" onClick={actions.cue} disabled={!meta}>CUE</button>
            <button
              className="btn-play"
              onClick={actions.togglePlay}
              disabled={!meta}
              style={{ background: `linear-gradient(180deg, ${color}, ${color}cc)`, boxShadow: playing ? `0 0 16px ${color}73` : "none" }}
            >
              <Icon name={playing ? "pause" : "play"} size={16} />
            </button>
          </div>
        </div>

        {/* controls */}
        <div className="controls-col">
          <div className="pads">
            {Array.from({ length: 8 }, (_, i) => {
              const set = state.hotCues[i] != null;
              const navOnly = !dsp && i >= 2; // streaming: only nav cues 1-2
              const c = CUE_COLORS[i % CUE_COLORS.length];
              return (
                <button
                  key={i}
                  className={`pad ${set ? "pad--set" : ""}`}
                  disabled={navOnly || !meta}
                  onClick={() => actions.setHotCue(i)}
                  onContextMenu={(e) => { e.preventDefault(); actions.clearHotCue(i); }}
                  style={set ? { color: c, borderColor: `${c}80`, background: `${c}26` } : undefined}
                  title={set ? "Jump (right-click clears)" : "Set hot cue"}
                >
                  {i + 1}
                </button>
              );
            })}
          </div>

          {dsp ? (
            <>
              <div className="chip-row">
                <button className="chip" disabled title="Loops: Phase 4">IN</button>
                <button className="chip" disabled title="Loops: Phase 4">OUT</button>
                <button className="chip chip--on" disabled title="Loops: Phase 4">4</button>
                <button className="chip" disabled title="Loops: Phase 4">8</button>
                <button className="chip" disabled title="Loops: Phase 4">16</button>
              </div>
              <div className="chip-row">
                <button className="chip" disabled title="FX rack: Phase 5">ECHO</button>
                <button className="chip" disabled title="FX rack: Phase 5">REVERB</button>
                <button className="chip" disabled title="FX rack: Phase 5">FILTER</button>
              </div>
              <p className="soon-note">Loops &amp; FX land in P4–P5.</p>
            </>
          ) : (
            <div className="stream-note">
              Loops &amp; FX disabled on stream decks — the service returns playback control, not
              decoded audio; compas won't fake DSP it can't perform.
            </div>
          )}
        </div>

        {/* tempo fader */}
        <div className="tempo-col">
          <button
            className="sync-btn"
            style={{ color, borderColor: `${color}66` }}
            onClick={onSync}
            disabled={!dsp || !syncEnabled}
            title="Match this deck's tempo to the other deck"
          >
            SYNC
          </button>
          <span className="overline">TEMPO</span>
          <Fader
            value={dsp ? pct : 0}
            min={-8}
            max={8}
            length={130}
            center
            color={color}
            disabled={!dsp || !meta}
            onChange={(v) => actions.setTempo(1 + v / 100)}
          />
          <span className="mono tempo-val" style={{ opacity: dsp ? 1 : 0.45 }}>
            {dsp ? `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}` : "0.0"}
            <small>%</small>
          </span>
          {dsp && (
            <div className="nudge">
              <button onPointerDown={() => actions.nudge(-1, true)} onPointerUp={() => actions.nudge(-1, false)} onPointerLeave={() => actions.nudge(-1, false)}>−</button>
              <button onPointerDown={() => actions.nudge(1, true)} onPointerUp={() => actions.nudge(1, false)} onPointerLeave={() => actions.nudge(1, false)}>+</button>
            </div>
          )}
        </div>
      </div>
      {/* EQ/gain knobs live in the mixer channel strip per the design. */}
    </section>
  );
}
