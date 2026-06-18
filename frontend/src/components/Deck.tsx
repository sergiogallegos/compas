import { useEffect, useRef, useState } from "react";
import type { DeckController } from "../hooks/useDeck";
import { Fader } from "./Fader";
import { Knob } from "./Knob";
import { Icon } from "./icons";

/// Beat-synced echo time choices, with compact labels.
const ECHO_BEATS: { v: number; label: string }[] = [
  { v: 0.25, label: "¼" },
  { v: 0.5, label: "½" },
  { v: 1, label: "1" },
  { v: 2, label: "2" },
];

const CUE_COLORS = ["var(--accent)", "var(--stream)", "var(--status-warn)", "var(--status-ok)"];

/// Degrees of platter rotation per second of audio (≈33⅓ RPM). Matches the play-head
/// `spin` mapping so a scratch gesture converts cleanly to a read-rate: speed = ω / this.
const DEG_PER_SEC = 200;

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

  // --- Jog-wheel scratch ---------------------------------------------------------
  // Dragging the platter streams a read-rate to the engine derived from angular
  // velocity, and rotates the disc 1:1 with the hand. A rAF loop samples the pointer
  // so a held-but-still finger decays to a clean "held" (speed 0) instead of coasting.
  const platterRef = useRef<HTMLDivElement>(null);
  const [scratching, setScratching] = useState(false);
  const [dragAngle, setDragAngle] = useState(0);
  const jog = useRef({
    active: false,
    rafId: 0,
    prevAngle: 0,
    curAngle: 0,
    lastTick: 0,
    smooth: 0,
    lastSent: 0,
    angle: 0,
  });

  const angleOf = (clientX: number, clientY: number): number => {
    const el = platterRef.current;
    if (!el) return 0;
    const r = el.getBoundingClientRect();
    return (Math.atan2(clientY - (r.top + r.height / 2), clientX - (r.left + r.width / 2)) * 180) / Math.PI;
  };

  const tick = (now: number) => {
    const j = jog.current;
    if (!j.active) return;
    let dt = (now - j.lastTick) / 1000;
    dt = dt <= 0 ? 1 / 60 : Math.min(dt, 0.05);
    let dTheta = j.curAngle - j.prevAngle;
    if (dTheta > 180) dTheta -= 360;
    else if (dTheta < -180) dTheta += 360;
    const raw = dTheta / dt / DEG_PER_SEC; // ω → read-rate (1.0 = natural play speed)
    j.smooth += (raw - j.smooth) * 0.5;
    if (Math.abs(j.smooth) < 0.01) j.smooth = 0; // snap a near-still finger to a clean hold
    if (Math.abs(j.smooth - j.lastSent) > 0.01 || (j.smooth === 0 && j.lastSent !== 0)) {
      actions.scratch(true, j.smooth);
      j.lastSent = j.smooth;
    }
    j.angle += dTheta; // disc follows the hand exactly
    setDragAngle(j.angle);
    j.prevAngle = j.curAngle;
    j.lastTick = now;
    j.rafId = requestAnimationFrame(tick);
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (!meta || !dsp) return;
    e.preventDefault();
    platterRef.current?.setPointerCapture(e.pointerId);
    const a = angleOf(e.clientX, e.clientY);
    const j = jog.current;
    j.active = true;
    j.prevAngle = a;
    j.curAngle = a;
    j.lastTick = performance.now();
    j.smooth = 0;
    j.lastSent = 0;
    j.angle = spin; // start from the current visual position to avoid a jump
    setScratching(true);
    actions.scratch(true, 0);
    j.rafId = requestAnimationFrame(tick);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (jog.current.active) jog.current.curAngle = angleOf(e.clientX, e.clientY);
  };

  const endScratch = (e: React.PointerEvent) => {
    const j = jog.current;
    if (!j.active) return;
    j.active = false;
    if (j.rafId) cancelAnimationFrame(j.rafId);
    j.rafId = 0;
    try {
      platterRef.current?.releasePointerCapture(e.pointerId);
    } catch {
      /* pointer already released */
    }
    setScratching(false);
    actions.scratch(false, 0);
  };

  // Stop the loop and release scratch if the deck unmounts mid-gesture.
  useEffect(() => {
    return () => {
      const j = jog.current;
      if (j.active) {
        j.active = false;
        if (j.rafId) cancelAnimationFrame(j.rafId);
        actions.scratch(false, 0);
      }
    };
    // actions is stable for the lifetime of the deck; run cleanup only on unmount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
          <div
            className={`platter${scratching ? " platter--scratch" : ""}`}
            ref={platterRef}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endScratch}
            onPointerCancel={endScratch}
            title={dsp ? "Drag to scratch / nudge" : undefined}
          >
            <div
              className="platter-ring"
              style={{ background: `conic-gradient(${color} ${frac * 360}deg, rgba(255,255,255,.07) 0)` }}
            />
            <div className="platter-disc" style={{ transform: `rotate(${scratching ? dragAngle : spin}deg)` }}>
              <div className="platter-marker" style={{ background: color, boxShadow: `0 0 8px ${color}` }} />
            </div>
            <div className="platter-center">
              <span className="mono platter-bpm">{meta && effBpm > 0 ? effBpm.toFixed(1) : "—"}</span>
              <span className="overline" style={{ color: scratching || playing ? color : "var(--text-tertiary)" }}>
                {scratching ? "SCRATCH" : playing ? "PLAYING" : "CUED"}
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
                <button className="chip" onClick={actions.loopIn} disabled={!meta} title="Set loop in">IN</button>
                <button className="chip" onClick={actions.loopOut} disabled={!meta} title="Set loop out & enable">OUT</button>
                {[4, 8, 16].map((n) => (
                  <button
                    key={n}
                    className={`chip ${state.loop.active && state.loop.beats === n ? "chip--on" : ""}`}
                    onClick={() => actions.beatLoop(n)}
                    disabled={!meta || (meta.beat_interval_sec ?? 0) <= 0}
                    title={`${n}-beat loop`}
                  >
                    {n}
                  </button>
                ))}
                <button className="chip" onClick={actions.clearLoop} disabled={!state.loop.active} title="Exit loop">
                  ✕
                </button>
              </div>
              <div className="chip-row">
                <button
                  className={`chip ${state.echo.active ? "chip--on" : ""}`}
                  onClick={actions.toggleEcho}
                  disabled={!meta}
                  title="Echo / delay"
                >
                  ECHO
                </button>
                <button
                  className={`chip ${state.reverb.active ? "chip--on" : ""}`}
                  onClick={actions.toggleReverb}
                  disabled={!meta}
                  title="Reverb"
                >
                  REVERB
                </button>
                <button className="chip" disabled title="The filter is the mixer's HPF/LPF knob">FILTER</button>
              </div>
              {state.echo.active && (
                <div className="fx-detail">
                  <div className="fx-beats">
                    {ECHO_BEATS.map((b) => (
                      <button
                        key={b.v}
                        className={`chip ${state.echo.beats === b.v ? "chip--on" : ""}`}
                        onClick={() => actions.setEchoBeats(b.v)}
                        title={`${b.label} beat`}
                      >
                        {b.label}
                      </button>
                    ))}
                  </div>
                  <Knob value={state.echo.depth} min={0} max={1} onChange={actions.setEchoDepth} label="DEPTH" color={color} size={34} />
                </div>
              )}
              {state.reverb.active && (
                <div className="fx-detail">
                  <span className="overline fx-label">REVERB</span>
                  <Knob value={state.reverb.size} min={0} max={1} onChange={actions.setReverbSize} label="SIZE" color={color} size={34} />
                  <Knob value={state.reverb.mix} min={0} max={1} onChange={actions.setReverbMix} label="MIX" color={color} size={34} />
                </div>
              )}
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
              <button onClick={() => actions.trimTempo(-1)} disabled={!meta} title="Nudge tempo −0.1%">−</button>
              <button onClick={() => actions.trimTempo(1)} disabled={!meta} title="Nudge tempo +0.1%">+</button>
            </div>
          )}
        </div>
      </div>
      {/* EQ/gain knobs live in the mixer channel strip per the design. */}
    </section>
  );
}
