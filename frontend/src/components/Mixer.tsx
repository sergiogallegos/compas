import { useState } from "react";
import type { DeckController } from "../hooks/useDeck";
import { Knob } from "./Knob";
import { Meter } from "./Meter";
import { Icon } from "./icons";

interface Channel {
  ctrl: DeckController;
  letter: string;
  color: string;
}

export function Mixer({
  channels,
  crossfader,
  onCrossfader,
}: {
  channels: Channel[];
  crossfader: number;
  onCrossfader: (v: number) => void;
}) {
  return (
    <section className="mixer">
      <div className="mixer-head">
        <span className="overline">MIXER</span>
        <span className="overline" style={{ color: "var(--text-tertiary)" }}>2-CH</span>
      </div>
      <div className="mixer-strips">
        {channels.map((c) => (
          <ChannelStrip key={c.letter} {...c} />
        ))}
      </div>
      <div className="xfader">
        <span className="overline" style={{ color: "var(--accent)" }}>A</span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={crossfader}
          onChange={(e) => onCrossfader(Number(e.target.value))}
        />
        <span className="overline" style={{ color: "var(--stream)" }}>B</span>
      </div>
    </section>
  );
}

function ChannelStrip({ ctrl, letter, color }: Channel) {
  const { state, actions } = ctrl;
  const dsp = state.dsp;
  const [trim, setTrim] = useState(1);
  const [fader, setFader] = useState(1);

  const applyGain = (t: number, f: number) => {
    setTrim(t);
    setFader(f);
    actions.setGain(t * f);
  };

  return (
    <div className="strip">
      <span className="strip-letter display" style={{ color }}>{letter}</span>

      <div className={`knob-stack ${dsp ? "" : "knob-stack--locked"}`}>
        <Knob label="GAIN" value={trim} min={0} max={1.5} color={color} disabled={!dsp}
          onChange={(v) => applyGain(v, fader)} />
        <Knob label="HI" value={state.eq.hi} min={-26} max={6} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, hi: v })} />
        <Knob label="MID" value={state.eq.mid} min={-26} max={6} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, mid: v })} />
        <Knob label="LOW" value={state.eq.low} min={-26} max={6} disabled={!dsp}
          onChange={(v) => actions.setEq({ ...state.eq, low: v })} />
        {!dsp && <span className="eq-na">EQ N/A</span>}
      </div>

      <div className="strip-meterfader">
        <Meter level={state.level} streaming={!dsp} height={150} />
        <input
          className="ch-fader"
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={fader}
          onChange={(e) => applyGain(trim, Number(e.target.value))}
          style={{ accentColor: color }}
        />
      </div>

      <button className="cue-btn" disabled title="Headphone cue bus — a later phase">
        <Icon name="headphones" size={14} />
      </button>
    </div>
  );
}
