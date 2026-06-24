import { memo, useEffect, useRef, useState, type DragEvent, type ReactElement } from "react";
import type { DeckController, DeckState } from "../hooks/useDeck";
import { bandColor, formatKey, loadTrack, type KeyNotation } from "../lib/ipc";

// drag-and-drop MIME carrying a track path from the library onto a deck
const TRACK_DND = "application/x-compas-track";
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

/// Flanger LFO sweep period in beats (beat-synced).
const FLANGER_BEATS: { v: number; label: string }[] = [
  { v: 1, label: "1" },
  { v: 2, label: "2" },
  { v: 4, label: "4" },
  { v: 8, label: "8" },
];

/// Loop-roll sizes (held, in beats), with compact labels.
const ROLLS: { v: number; label: string }[] = [
  { v: 0.125, label: "⅛" },
  { v: 0.25, label: "¼" },
  { v: 0.5, label: "½" },
];

// Per-slot hot-cue colors (Serato-style 8-color palette). Hex so an alpha suffix (`${c}30`) is
// valid CSS — a CSS var like `var(--accent)` can't take an appended alpha.
const CUE_COLORS = ["#ff5b4c", "#ff8c1a", "#ffcf3a", "#3ddc97", "#28e0ff", "#2ea6ff", "#9b6bff", "#ff5bbf"];

// Seconds of track remaining at which the deck flashes the Pioneer-style end-of-track warning
// (platter ring + PLAY button). TODO: surface as a user setting (CDJs make this configurable).
const END_WARNING_SECS = 30;

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
  syncActive = false,
  keyNotation = "camelot",
  compact = false,
  mirror = false,
  slots,
}: {
  ctrl: DeckController;
  color: string;
  onSync?: () => void;
  syncEnabled?: boolean;
  syncActive?: boolean;
  /** Camelot (8A) vs musical (C#m) key display. */
  keyNotation?: KeyNotation;
  /** Condensed variant for 4-deck mode (hides the jog platter, tightens spacing). */
  compact?: boolean;
  /** Mirror the deck so the platter sits on the inner edge (next to the mixer). */
  mirror?: boolean;
  /** Deck-select tabs for this slot (e.g. A/C or B/D). */
  slots?: { label: string; active: boolean; onSelect: () => void }[];
}) {
  const { state, actions } = ctrl;
  const { meta, frame, playing, tempo, dsp, loading } = state;
  const rate = meta?.source_rate ?? 1;
  const frames = meta?.frames ?? 0;
  const noGrid = !meta || (meta.beat_interval_sec ?? 0) <= 0;

  // Loop-roll is momentary: held while the pad is pressed, released on pointer up/leave.
  const [rollBeats, setRollBeats] = useState<number | null>(null);
  const rollRef = useRef<number | null>(null);
  const startRoll = (b: number) => {
    rollRef.current = b;
    setRollBeats(b);
    actions.loopRoll(b, true);
  };
  const endRoll = () => {
    if (rollRef.current == null) return;
    rollRef.current = null;
    setRollBeats(null);
    actions.loopRoll(0, false);
  };
  const frac = frames > 0 ? Math.min(1, frame / frames) : 0;
  const effBpm = meta ? meta.bpm * tempo : 0;
  const spin = (((frame / rate) * 200) % 360 + 360) % 360;

  // End-of-track warning: source-time remaining (natural-rate seconds, like the TIME readout).
  // Flashes only while playing so a deck parked near the end doesn't blink forever.
  const remainingSecs = frames > 0 ? (frames - frame) / rate : Infinity;
  const endWarning = !!meta && playing && remainingSecs > 0 && remainingSecs <= END_WARNING_SECS;

  // Manual loop-in is set and waiting for OUT — light IN, invite OUT.
  const loopArmed = !!state.loop.armed && !state.loop.active;

  // tempo fader works in percent for display; ratio for the engine.
  const pct = (tempo - 1) * 100;

  // --- Jog-wheel scratch ---------------------------------------------------------
  // Dragging the platter streams a read-rate to the engine derived from angular
  // velocity, and rotates the disc 1:1 with the hand. A rAF loop samples the pointer
  // so a held-but-still finger decays to a clean "held" (speed 0) instead of coasting.
  const platterRef = useRef<HTMLDivElement>(null);
  const [scratching, setScratching] = useState(false);
  const [dragAngle, setDragAngle] = useState(0);
  // Library → deck drag-and-drop: highlight while a track hovers, load it on drop.
  const [dropActive, setDropActive] = useState(false);
  const onTrackDragOver = (e: DragEvent) => {
    if (!e.dataTransfer.types.includes(TRACK_DND)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    setDropActive(true);
  };
  const onTrackDrop = (e: DragEvent) => {
    if (!e.dataTransfer.types.includes(TRACK_DND)) return;
    e.preventDefault();
    setDropActive(false);
    const path = e.dataTransfer.getData(TRACK_DND);
    if (path) loadTrack(ctrl.deck, path).catch(() => {});
  };
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

  // Release scratch and any held loop-roll if the deck unmounts mid-gesture.
  useEffect(() => {
    return () => {
      const j = jog.current;
      if (j.active) {
        j.active = false;
        if (j.rafId) cancelAnimationFrame(j.rafId);
        actions.scratch(false, 0);
      }
      if (rollRef.current != null) {
        rollRef.current = null;
        actions.loopRoll(0, false);
      }
    };
    // actions is stable for the lifetime of the deck; run cleanup only on unmount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <section
      className={`deck${mirror ? " deck--mirror" : ""}${compact ? " deck--compact" : ""}`}
      style={{
        borderTopColor: color,
        ...(dropActive ? { outline: `2px dashed ${color}`, outlineOffset: "-4px", background: `${color}10` } : null),
      }}
      onDragOver={onTrackDragOver}
      onDragLeave={() => setDropActive(false)}
      onDrop={onTrackDrop}
    >
      {/* header */}
      <div className="deck-header">
        {slots && (
          <div className="deck-slots">
            {slots.map((s) => (
              <button
                key={s.label}
                className={`deck-slot ${s.active ? "deck-slot--on" : ""}`}
                style={s.active ? { color, borderColor: color, background: `${color}22` } : undefined}
                onClick={s.onSelect}
                title={`Control deck ${s.label} in this slot`}
              >
                {s.label}
              </button>
            ))}
          </div>
        )}
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
            {meta?.key_camelot ? formatKey(meta.key_camelot, meta.key_name, keyNotation) : "—"}
          </span>
        </div>
        <div className="tile">
          <span className="overline">TIME</span>
          <span className="mono tile-val small">
            {fmt(frame, rate)} <span className="muted">/ -{fmt(frames - frame, rate)}</span>
          </span>
        </div>
      </div>

      {/* full-track overview (rekordbox-style): summary waveform, hot-cue/loop markers,
          and click/drag-to-seek across the whole track. */}
      <OverviewBar state={state} color={color} onSeek={actions.seekFrac} />

      <div className="deck-body">
        {/* platter */}
        <div className="platter-col">
          <div
            className={`platter${scratching ? " platter--scratch" : ""}${endWarning ? " platter--warn" : ""}`}
            ref={platterRef}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endScratch}
            onPointerCancel={endScratch}
            title={dsp ? "Drag to scratch / nudge" : undefined}
          >
            <div
              className="platter-ring"
              style={{
                // Position ring: elapsed in the deck color, the remaining track kept visible (not
                // near-invisible) so the whole circle reads as song progress, CDJ-style.
                background: `conic-gradient(${color} ${frac * 360}deg, rgba(255,255,255,.16) 0)`,
              }}
            />
            <div className="platter-disc" style={{ transform: `rotate(${scratching ? dragAngle : spin}deg)` }}>
              <div className="platter-marker" style={{ background: color, boxShadow: `0 0 8px ${color}` }} />
            </div>
            <div className="platter-center">
              <span className="mono platter-bpm">{meta && effBpm > 0 ? effBpm.toFixed(1) : "—"}</span>
              <span className="overline" style={{ color: endWarning ? "var(--cue)" : scratching || playing ? color : "var(--text-tertiary)" }}>
                {scratching ? "SCRATCH" : endWarning ? "ENDING" : playing ? "PLAYING" : "CUED"}
              </span>
            </div>
          </div>
          <div className="platter-btns">
            {dsp && (
              <button
                className="chip cue-mode"
                onClick={() => actions.setCueMode(state.cueMode === 0 ? 1 : 0)}
                disabled={!meta}
                title="Cue button mode: CDJ (preview-while-held) vs gated (stutter)"
              >
                {state.cueMode === 1 ? "GATE" : "CDJ"}
              </button>
            )}
            <button
              className="btn-cue"
              onPointerDown={() => dsp && actions.cueButton(true)}
              onPointerUp={() => dsp && actions.cueButton(false)}
              onPointerLeave={() => dsp && actions.cueButton(false)}
              onClick={() => !dsp && actions.cue()}
              disabled={!meta}
              title="Main cue (hold to preview)"
            >
              CUE
            </button>
            <button
              className={`btn-play${playing ? " btn-play--on" : meta ? " btn-play--ready" : ""}${endWarning ? " btn-play--warn" : ""}`}
              onClick={actions.togglePlay}
              disabled={!meta}
              title={playing ? "Pause" : meta ? "Play (ready)" : "Play"}
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
                  style={set ? { color: "#fff", borderColor: c, background: `${c}33`, boxShadow: `0 0 7px ${c}66, inset 0 0 0 1px ${c}` } : undefined}
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
                <button
                  className={`chip chip--round ${loopArmed ? "chip--armed" : ""}`}
                  onClick={actions.loopIn}
                  disabled={!meta}
                  title="Set loop-in point (then press OUT to close the loop)"
                >
                  IN
                </button>
                <button
                  className={`chip chip--round ${state.loop.active ? "chip--loopon" : loopArmed ? "chip--armflash" : ""}`}
                  onClick={actions.loopOut}
                  disabled={!meta}
                  title={loopArmed ? "Set loop-out & enable the loop" : "Set loop out & enable"}
                >
                  OUT
                </button>
                {[4, 8, 16].map((n) => (
                  <button
                    key={n}
                    className={`chip ${state.loop.active && state.loop.beats === n ? "chip--loopon" : ""}`}
                    onClick={() => actions.beatLoop(n)}
                    disabled={!meta || (meta.beat_interval_sec ?? 0) <= 0}
                    title={`${n}-beat loop`}
                  >
                    {n}
                  </button>
                ))}
                <button className="chip" onClick={() => actions.scaleLoop(0.5)} disabled={!state.loop.active} title="Halve loop length">
                  ½×
                </button>
                <button className="chip" onClick={() => actions.scaleLoop(2)} disabled={!state.loop.active} title="Double loop length">
                  2×
                </button>
                <button className="chip" onClick={() => actions.moveLoop(-1)} disabled={!state.loop.active} title="Move loop back 1 beat">
                  ◀
                </button>
                <button className="chip" onClick={() => actions.moveLoop(1)} disabled={!state.loop.active} title="Move loop forward 1 beat">
                  ▶
                </button>
                <button className="chip" onClick={actions.clearLoop} disabled={!state.loop.active} title="Exit loop">
                  ✕
                </button>
              </div>
              <div className="chip-row">
                <button
                  className={`chip ${state.quantize ? "chip--on" : ""}`}
                  onClick={actions.toggleQuantize}
                  disabled={!meta}
                  title="Quantize: snap cue jumps & beat-jumps to the grid"
                >
                  Q
                </button>
                <button className="chip" onClick={() => actions.beatJump(-4)} disabled={noGrid} title="Jump back 4 beats">
                  ◀4
                </button>
                <button className="chip" onClick={() => actions.beatJump(4)} disabled={noGrid} title="Jump forward 4 beats">
                  4▶
                </button>
                {ROLLS.map((r) => (
                  <button
                    key={r.v}
                    className={`chip ${rollBeats === r.v ? "chip--on" : ""}`}
                    onPointerDown={(e) => { e.preventDefault(); startRoll(r.v); }}
                    onPointerUp={endRoll}
                    onPointerLeave={endRoll}
                    onPointerCancel={endRoll}
                    disabled={noGrid}
                    title={`${r.label}-beat loop roll (hold)`}
                  >
                    {r.label}
                  </button>
                ))}
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
                <button
                  className={`chip ${state.flanger.active ? "chip--on" : ""}`}
                  onClick={actions.toggleFlanger}
                  disabled={!meta}
                  title="Flanger (beat-synced sweep)"
                >
                  FLANGE
                </button>
                <button
                  className={`chip ${state.crusher.active ? "chip--on" : ""}`}
                  onClick={actions.toggleCrusher}
                  disabled={!meta}
                  title="Bitcrusher (bit-depth + sample-rate reduction)"
                >
                  CRUSH
                </button>
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
              {state.flanger.active && (
                <div className="fx-detail">
                  <div className="fx-beats">
                    {FLANGER_BEATS.map((b) => (
                      <button
                        key={b.v}
                        className={`chip ${state.flanger.beats === b.v ? "chip--on" : ""}`}
                        onClick={() => actions.setFlangerBeats(b.v)}
                        title={`${b.label}-beat sweep`}
                      >
                        {b.label}
                      </button>
                    ))}
                  </div>
                  <Knob value={state.flanger.depth} min={0} max={1} onChange={actions.setFlangerDepth} label="DEPTH" color={color} size={34} />
                </div>
              )}
              {state.crusher.active && (
                <div className="fx-detail">
                  <span className="overline fx-label">CRUSH</span>
                  <Knob value={state.crusher.crush} min={0} max={1} onChange={actions.setCrusherCrush} label="BITS" color={color} size={34} />
                  <Knob value={state.crusher.down} min={0} max={1} onChange={actions.setCrusherDown} label="RATE" color={color} size={34} />
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
            className={`sync-btn ${syncActive ? "sync-btn--on" : ""}`}
            style={syncActive ? { color: "#fff", borderColor: color, background: `${color}cc` } : { color, borderColor: `${color}66` }}
            onClick={onSync}
            disabled={!dsp || (!syncEnabled && !syncActive)}
            title="Continuous beat-sync to the other deck (tempo + phase)"
          >
            SYNC
          </button>
          {dsp && (
            <button
              className={`chip mic-sync ${state.syncLive ? "chip--on" : ""}`}
              style={{ flex: "none", width: "100%", padding: "3px 0", fontSize: 9 }}
              onClick={actions.toggleSyncLive}
              disabled={!meta}
              title="Beat-match this deck to the live mic/aux input (tempo). Turn AUX on first."
            >
              MIC
            </button>
          )}
          {dsp && (
            <button
              className={`chip int-sync ${state.syncInternal ? "chip--on" : ""}`}
              style={{ flex: "none", width: "100%", padding: "3px 0", fontSize: 9 }}
              onClick={actions.toggleSyncInternal}
              disabled={!meta}
              title="Beat-match this deck to the internal master clock. Turn INT CLK on first."
            >
              INT
            </button>
          )}
          {dsp && (
            <div className="sync-opts">
              <button
                className={`chip ${state.syncMode === 1 ? "chip--on" : ""}`}
                onClick={() => actions.setSyncMode(state.syncMode === 1 ? 0 : 1)}
                disabled={!meta}
                title="Tempo-only sync (match BPM without locking phase)"
              >
                TEMPO
              </button>
              <button
                className={`chip ${state.isLeader ? "chip--on" : ""}`}
                onClick={actions.toggleLeader}
                disabled={!meta}
                title="Pin this deck as the sync leader"
              >
                LEAD
              </button>
            </div>
          )}
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
          {dsp && (
            <button
              className={`keylock-btn ${state.keylock ? "keylock-btn--on" : ""}`}
              style={state.keylock ? { color, borderColor: `${color}80` } : undefined}
              onClick={actions.toggleKeylock}
              disabled={!meta}
              title="Key-lock: change tempo without changing pitch (master tempo)"
            >
              {state.keylock ? "🔒 KEY" : "KEY"}
            </button>
          )}
        </div>
      </div>
      {/* EQ/gain knobs live in the mixer channel strip per the design. */}
    </section>
  );
}

/// Full-track overview waveform. Rendered once per loaded track (memoized on the peaks
/// identity) so the frequent play-head/position updates don't re-rasterize the whole
/// summary every telemetry tick — the moving parts (head, played scrim, markers) are cheap
/// absolutely-positioned divs layered on top.
const OverviewWave = memo(function OverviewWave({
  peaks,
  bands,
  color,
}: {
  peaks: number[];
  bands: [number, number, number][];
  color: string;
}) {
  const W = 600;
  const H = 100;
  const cy = H / 2;
  const N = peaks.length;
  const useBands = bands.length === N && N > 0;
  // Cap at ~one bar per output column so long tracks stay cheap to draw.
  const cols = Math.min(N, W);
  const step = Math.max(1, Math.ceil(N / cols));
  const bw = Math.max(0.8, W / Math.ceil(N / step));
  const bars: ReactElement[] = [];
  for (let i = 0; i < N; i += step) {
    let pk = 0;
    let bi = i;
    const end = Math.min(N, i + step);
    for (let j = i; j < end; j++) {
      if (peaks[j] > pk) {
        pk = peaks[j];
        bi = j;
      }
    }
    const x = (i / N) * W;
    const amp = Math.min(1, pk) * cy * 0.94;
    bars.push(
      <line key={i} x1={x} x2={x} y1={cy - amp} y2={cy + amp} stroke={useBands ? bandColor(bands[bi]) : color} strokeWidth={bw} />,
    );
  }
  return (
    <svg className="ov-svg" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none">
      {bars}
    </svg>
  );
});

function OverviewBar({ state, color, onSeek }: { state: DeckState; color: string; onSeek: (frac: number) => void }) {
  const meta = state.meta;
  const frames = meta?.frames ?? 0;
  const peaks = meta?.peaks ?? [];
  const dragging = useRef(false);

  if (!meta || peaks.length === 0 || frames <= 0) {
    return <div className="overview overview--empty" />;
  }

  const frac = Math.min(1, Math.max(0, state.frame / frames));
  const seekAt = (clientX: number, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    onSeek(Math.min(1, Math.max(0, (clientX - r.left) / r.width)));
  };
  const loop = state.loop;
  const loopLeft = loop.active ? Math.max(0, loop.inFrame / frames) : 0;
  const loopRight = loop.active ? Math.min(1, loop.outFrame / frames) : 0;

  return (
    <div
      className="overview"
      title="Click or drag to seek"
      onPointerDown={(e) => {
        dragging.current = true;
        e.currentTarget.setPointerCapture(e.pointerId);
        seekAt(e.clientX, e.currentTarget);
      }}
      onPointerMove={(e) => {
        if (dragging.current) seekAt(e.clientX, e.currentTarget);
      }}
      onPointerUp={(e) => {
        dragging.current = false;
        try {
          e.currentTarget.releasePointerCapture(e.pointerId);
        } catch {
          /* already released */
        }
      }}
      onPointerCancel={() => {
        dragging.current = false;
      }}
    >
      <OverviewWave peaks={peaks} bands={meta.band_peaks ?? []} color={color} />
      {/* dim the portion already played so what's coming reads bright */}
      <div className="ov-played" style={{ width: `${frac * 100}%` }} />
      {loop.active && loopRight > loopLeft && (
        <div className="ov-loop" style={{ left: `${loopLeft * 100}%`, width: `${(loopRight - loopLeft) * 100}%`, background: `${color}26`, borderColor: color }} />
      )}
      {state.hotCues.map((c, i) =>
        c == null ? null : (
          <div key={i} className="ov-cue" style={{ left: `${Math.min(100, (c / frames) * 100)}%`, background: CUE_COLORS[i % CUE_COLORS.length] }} />
        ),
      )}
      <div className="ov-head" style={{ left: `${frac * 100}%`, background: color, boxShadow: `0 0 6px ${color}` }} />
    </div>
  );
}
