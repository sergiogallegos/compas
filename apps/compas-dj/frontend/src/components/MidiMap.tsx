import { useMemo } from "react";
import type { MidiApi } from "../hooks/useMidi";
import type { MidiMapApi, MidiTarget } from "../hooks/useMidiMap";
import { mpkMiniMk3Profile, sourceLabel } from "../lib/midiMap";

/**
 * MIDI control-mapping panel: connect a controller, then bind any knob/pad to a deck or
 * mixer control by clicking Learn and moving it. Bindings persist to localStorage.
 */
export function MidiMap({
  midi,
  map,
  onClose,
}: {
  midi: MidiApi;
  map: MidiMapApi;
  onClose: () => void;
}) {
  // Preserve registry order while grouping (Deck A–D, then Global).
  const groups = useMemo(() => {
    const order: string[] = [];
    const byGroup = new Map<string, MidiTarget[]>();
    for (const t of map.targets) {
      if (!byGroup.has(t.group)) {
        byGroup.set(t.group, []);
        order.push(t.group);
      }
      byGroup.get(t.group)!.push(t);
    }
    return order.map((g) => [g, byGroup.get(g)!] as const);
  }, [map.targets]);

  const Row = (t: MidiTarget) => {
    const sources = map.sourcesFor(t.id);
    const learning = map.learning === t.id;
    return (
      <div key={t.id} className={`mm-row ${learning ? "mm-row--learn" : ""}`}>
        <span className="mm-label">{t.label}</span>
        <span className="mono mm-binding">
          {learning ? "move a control…" : sources.length ? sources.map(sourceLabel).join(", ") : "—"}
        </span>
        <button
          className={`chip mm-learn ${learning ? "chip--on" : ""}`}
          onClick={() => (learning ? map.cancelLearn() : map.startLearn(t.id))}
        >
          {learning ? "CANCEL" : "LEARN"}
        </button>
        <button
          className="chip mm-clear"
          disabled={!sources.length}
          onClick={() => map.clearTarget(t.id)}
          title="Clear binding"
        >
          ✕
        </button>
      </div>
    );
  };

  return (
    <div className="midimap">
      <div className="midimap-bar">
        <span className="overline">MIDI MAPPING</span>
        <div className="mm-device">
          <select
            value={midi.portIdx}
            onChange={(e) => midi.setPortIdx(Number(e.target.value))}
            disabled={!!midi.connected}
          >
            {midi.ports.length ? (
              midi.ports.map((p, i) => (
                <option key={i} value={i}>
                  {p}
                </option>
              ))
            ) : (
              <option>No MIDI devices</option>
            )}
          </select>
          <button className={`chip ${midi.connected ? "chip--on" : ""}`} onClick={midi.toggle}>
            {midi.connected ? "MIDI ✓" : midi.ports.length ? "CONNECT" : "RESCAN"}
          </button>
        </div>
        {map.lastSource && <span className="mono mm-last">{sourceLabel(map.lastSource)}</span>}
        <div className="mm-actions">
          <button className="chip" onClick={() => map.setBindings(mpkMiniMk3Profile())} title="Load Akai MPK Mini MK3 starter profile">
            MPK MK3
          </button>
          <button className="chip" onClick={map.clearAll} title="Clear all bindings">
            CLEAR
          </button>
        </div>
        <button className="chip mm-close" onClick={onClose} title="Close">
          ✕
        </button>
      </div>
      <div className="midimap-body">
        {groups.map(([group, targets], gi) => (
          <details key={group} className="mm-group" open={gi < 2 || group === "Global"}>
            <summary className="mm-group-head overline">{group}</summary>
            <div className="mm-rows">{targets.map(Row)}</div>
          </details>
        ))}
      </div>
    </div>
  );
}
