import { useState } from "react";
import type { DeckController } from "../hooks/useDeck";
import type { CueApi } from "../hooks/useCue";
import { Knob } from "./Knob";
import { Fader } from "./Fader";
import { Meter } from "./Meter";
import { Icon } from "./icons";

interface Channel {
  ctrl: DeckController;
  letter: string;
  color: string;
}

interface AutoMixProps {
  enabled: boolean;
  transitioning: boolean;
  onToggle: () => void;
  onMixNow: () => void;
}

interface XfaderConfig {
  curve: number;
  additive: boolean;
  reverse: boolean;
  onChange: (curve: number, additive: boolean, reverse: boolean) => void;
}

export function Mixer({
  channels,
  crossfader,
  onCrossfader,
  xfader,
  onFxMacro,
  auto,
  cue,
}: {
  channels: Channel[];
  crossfader: number;
  onCrossfader: (v: number) => void;
  xfader?: XfaderConfig;
  onFxMacro?: (deck: number, v: number) => void;
  auto?: AutoMixProps;
  cue?: CueApi;
}) {
  return (
    <section className="mixer">
      <div className="mixer-head">
        <span className="overline">MIXER · {channels.length}CH</span>
        {auto && (
          <div className="automix">
            <button
              className={`chip ${auto.enabled ? "chip--on" : ""}`}
              onClick={auto.onToggle}
              title="Auto-mix: beatmatch + bass-swap crossfade into the other deck near track end"
            >
              AUTO
            </button>
            <button
              className="chip"
              onClick={auto.onMixNow}
              disabled={auto.transitioning}
              title="Transition to the other deck now"
            >
              {auto.transitioning ? "MIXING…" : "MIX"}
            </button>
          </div>
        )}
      </div>
      <div className="mixer-strips">
        {channels.map((c) => (
          <ChannelStrip key={c.letter} {...c} cue={cue} onFxMacro={onFxMacro} />
        ))}
      </div>
      <div className="xfader">
        <span className="overline" style={{ color: "var(--accent)" }}>A</span>
        <Fader
          value={crossfader}
          min={0}
          max={1}
          onChange={onCrossfader}
          orientation="horizontal"
          fill
          center
          color="var(--text-primary)"
        />
        <span className="overline" style={{ color: "var(--stream)" }}>B</span>
      </div>
      {xfader && <XfaderConfigRow {...xfader} />}
      {cue && <Phones cue={cue} />}
    </section>
  );
}

/** Crossfader response: curve steepness (smooth → cut), additive/cut mode, and reverse. */
function XfaderConfigRow({ curve, additive, reverse, onChange }: XfaderConfig) {
  return (
    <div className="xf-config">
      <span className="overline">CURVE</span>
      <input
        className="xf-curve"
        type="range"
        min={0.5}
        max={6}
        step={0.1}
        value={curve}
        onChange={(e) => onChange(parseFloat(e.target.value), additive, reverse)}
        title="Crossfader curve: low = smooth blend, high = sharp cut"
      />
      <button
        className={`chip ${additive ? "chip--on" : ""}`}
        onClick={() => onChange(curve, !additive, reverse)}
        title="Additive (slow-fade / fast-cut) vs constant-power"
      >
        CUT
      </button>
      <button
        className={`chip ${reverse ? "chip--on" : ""}`}
        onClick={() => onChange(curve, additive, !reverse)}
        title="Reverse the crossfader (hamster switch)"
      >
        REV
      </button>
    </div>
  );
}

/** Headphone cue master controls: output device, on/off, cue↔master blend, level. */
function Phones({ cue }: { cue: CueApi }) {
  return (
    <div className="phones">
      <Icon name="headphones" size={14} />
      <select
        className="phones-dev"
        value={cue.device ?? ""}
        onChange={(e) => cue.setDevice(e.target.value || null)}
        disabled={cue.enabled}
        title="Headphone output device"
      >
        <option value="">Default output</option>
        {cue.devices.map((d) => (
          <option key={d} value={d}>{d}</option>
        ))}
      </select>
      <button
        className={`chip phones-on ${cue.enabled ? "chip--on" : ""}`}
        onClick={cue.toggle}
        title={cue.enabled ? `Cue on: ${cue.connectedName ?? ""}` : "Start headphone cue output"}
      >
        {cue.enabled ? "ON" : "OFF"}
      </button>
      <Knob value={cue.mix} min={0} max={1} size={26} label="CUE◁▷MAS" onChange={cue.setMix} />
      <Knob value={cue.volume} min={0} max={1} size={26} label="PHONES" onChange={cue.setVolume} />
    </div>
  );
}

const XF_LABELS = ["A", "│", "B"]; // assign: 0 = A side, 1 = thru, 2 = B side

function ChannelStrip({
  ctrl,
  letter,
  color,
  cue,
  onFxMacro,
}: Channel & { cue?: CueApi; onFxMacro?: (deck: number, v: number) => void }) {
  const { state, actions } = ctrl;
  const dsp = state.dsp;
  const cued = !!cue?.cued.has(ctrl.deck);
  const [fxMacro, setFxMacro] = useState(0);
  // GAIN trim × channel fader both scale the single engine gain.
  const setVol = (trim: number, fader: number) => actions.setGain(trim * fader);

  return (
    <div className="strip">
      <span className="strip-letter display" style={{ color }}>{letter}</span>

      <div className="xf-assign" title="Crossfader assign: A side / thru / B side">
        {XF_LABELS.map((lbl, i) => (
          <button
            key={i}
            className={`xf-seg ${state.xfaderAssign === i ? "xf-seg--on" : ""}`}
            style={state.xfaderAssign === i ? { color, borderColor: color } : undefined}
            onClick={() => actions.setXfaderAssign(i)}
          >
            {lbl}
          </button>
        ))}
      </div>

      <div className={`knob-stack ${dsp ? "" : "knob-stack--locked"}`}>
        <Knob label="GAIN" value={state.gain} min={0} max={1.5} size={28} color={color} disabled={!dsp}
          onChange={(v) => actions.setGain(v)} />
        <Knob label="HI" value={state.eq.hi} min={-26} max={6} size={28} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, hi: v })} />
        <Knob label="MID" value={state.eq.mid} min={-26} max={6} size={28} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, mid: v })} />
        <Knob label="LOW" value={state.eq.low} min={-26} max={6} size={28} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, low: v })} />
        <Knob label="FILTER" value={state.filter} min={-1} max={1} size={28} color={color} disabled={!dsp}
          onChange={(v) => actions.setFilter(v)} />
        {onFxMacro && (
          <Knob label="FX" value={fxMacro} min={0} max={1} size={28} color={color} disabled={!dsp}
            onChange={(v) => { setFxMacro(v); onFxMacro(ctrl.deck, v); }} />
        )}
        {!dsp && <span className="eq-na">EQ N/A</span>}
      </div>

      <div className="strip-meterfader">
        <Meter level={state.level} streaming={!dsp} />
        <Fader value={state.gain} min={0} max={1.5} fill color={color}
          onChange={(v) => setVol(1, v)} />
      </div>

      <button
        className={`cue-btn ${cued ? "cue-btn--on" : ""}`}
        onClick={() => cue?.toggleDeckCue(ctrl.deck)}
        disabled={!cue}
        title="Pre-listen this deck in the headphones (PFL)"
      >
        <Icon name="headphones" size={14} />
      </button>
    </div>
  );
}
