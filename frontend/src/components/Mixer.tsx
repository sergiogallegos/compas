import type { DeckController } from "../hooks/useDeck";
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

export function Mixer({
  channels,
  crossfader,
  onCrossfader,
  auto,
}: {
  channels: Channel[];
  crossfader: number;
  onCrossfader: (v: number) => void;
  auto?: AutoMixProps;
}) {
  return (
    <section className="mixer">
      <div className="mixer-head">
        <span className="overline">MIXER</span>
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
          <ChannelStrip key={c.letter} {...c} />
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
    </section>
  );
}

function ChannelStrip({ ctrl, letter, color }: Channel) {
  const { state, actions } = ctrl;
  const dsp = state.dsp;
  // GAIN trim × channel fader both scale the single engine gain.
  const setVol = (trim: number, fader: number) => actions.setGain(trim * fader);

  return (
    <div className="strip">
      <span className="strip-letter display" style={{ color }}>{letter}</span>

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
        {!dsp && <span className="eq-na">EQ N/A</span>}
      </div>

      <div className="strip-meterfader">
        <Meter level={state.level} streaming={!dsp} />
        <Fader value={state.gain} min={0} max={1.5} fill color={color}
          onChange={(v) => setVol(1, v)} />
      </div>

      <button className="cue-btn" disabled title="Headphone cue bus — a later phase">
        <Icon name="headphones" size={14} />
      </button>
    </div>
  );
}
