import { useEffect, useMemo, useRef, useState } from "react";
import {
  controllerActivate,
  controllerList,
  controllerRegistry,
  controllerSave,
  onMidiCc,
  onMidiNote,
  type ControllerBinding,
  type ControllerProfile,
  type ControlSpec,
} from "../lib/ipc";

const slug = (s: string) =>
  s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "controller";

const inputLabel = (b: ControllerBinding) =>
  b.input.kind === "cc" ? `CC ${b.input.cc}` : `Note ${b.input.note}`;

/** Guided controller-mapping editor: capture a binding by wiggling a control on the hardware
 *  (the device's own MIDI defines the mapping — clean-room by construction), then save/activate
 *  it as a profile. Lists the engine's control-bus registry as the mappable targets. */
export function ControllerMap({ onClose }: { onClose: () => void }) {
  const [registry, setRegistry] = useState<ControlSpec[]>([]);
  const [profiles, setProfiles] = useState<ControllerProfile[]>([]);
  const [name, setName] = useState("My Controller");
  const [bindings, setBindings] = useState<Record<string, ControllerBinding>>({});
  const [learning, setLearning] = useState<string | null>(null);
  const [status, setStatus] = useState("");

  const learningRef = useRef<string | null>(null);
  learningRef.current = learning;

  useEffect(() => {
    controllerRegistry().then(setRegistry).catch(() => {});
    controllerList().then(setProfiles).catch(() => {});
  }, []);

  // Capture the next MIDI message into the control being learned.
  useEffect(() => {
    const capture = (input: ControllerBinding["input"], channel: number) => {
      const control = learningRef.current;
      if (!control) return;
      setBindings((b) => ({ ...b, [control]: { channel, input, control, soft_takeover: true } }));
      setLearning(null);
      setStatus(`Bound ${control}`);
    };
    const unCc = onMidiCc((e) => capture({ kind: "cc", cc: e.controller }, e.channel));
    const unNote = onMidiNote((e) => {
      if (e.on) capture({ kind: "note", note: e.note }, e.channel);
    });
    return () => {
      unCc.then((u) => u());
      unNote.then((u) => u());
    };
  }, []);

  const profile = useMemo<ControllerProfile>(
    () => ({ id: slug(name), name, bindings: Object.values(bindings) }),
    [name, bindings],
  );

  const save = async () => {
    try {
      await controllerSave(profile);
      setProfiles(await controllerList());
      setStatus(`Saved "${name}"`);
    } catch (e) {
      setStatus(`Save failed: ${e}`);
    }
  };
  const activate = async () => {
    try {
      await controllerActivate(profile);
      setStatus(`Activated "${name}" (${profile.bindings.length} bindings)`);
    } catch (e) {
      setStatus(`Activate failed: ${e}`);
    }
  };
  const loadProfile = (p: ControllerProfile) => {
    setName(p.name);
    const map: Record<string, ControllerBinding> = {};
    for (const b of p.bindings) map[b.control] = b;
    setBindings(map);
    setStatus(`Loaded "${p.name}"`);
  };
  const clearBinding = (control: string) =>
    setBindings((b) => {
      const next = { ...b };
      delete next[control];
      return next;
    });

  const boundCount = Object.keys(bindings).length;

  return (
    <div className="panel-overlay" onClick={onClose}>
      <div className="panel ctrl-map" onClick={(e) => e.stopPropagation()}>
        <header className="panel-head">
          <span className="display">Controller mapping</span>
          <button className="chip" onClick={onClose}>✕</button>
        </header>

        <div className="ctrl-toolbar">
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Profile name" />
          <button className="chip" onClick={save} disabled={!boundCount}>Save</button>
          <button className="chip chip--on" onClick={activate} disabled={!boundCount}>Activate</button>
          <span className="mono">{boundCount} bound</span>
        </div>

        {profiles.length > 0 && (
          <div className="ctrl-profiles">
            <span className="overline">PROFILES</span>
            {profiles.map((p) => (
              <button key={p.id} className="chip" onClick={() => loadProfile(p)} title="Load into the editor">
                {p.name}
              </button>
            ))}
          </div>
        )}

        <p className="ctrl-hint">
          Click <strong>Learn</strong> on a control, then move that knob/pad/fader on your controller.
          The device's own MIDI defines the mapping. Connect your controller in the MIDI panel first.
        </p>

        <div className="ctrl-list">
          {registry.map((c) => {
            const b = bindings[c.id];
            const isLearning = learning === c.id;
            return (
              <div key={c.id} className={`ctrl-row ${b ? "ctrl-row--bound" : ""}`}>
                <span className="ctrl-id mono">{c.id}</span>
                <span className="ctrl-label">{c.label}</span>
                <span className="ctrl-binding mono">{b ? `${inputLabel(b)} · ch${b.channel}` : "—"}</span>
                <button
                  className={`chip ${isLearning ? "chip--on" : ""}`}
                  onClick={() => setLearning(isLearning ? null : c.id)}
                >
                  {isLearning ? "Wiggle…" : "Learn"}
                </button>
                <button className="chip" onClick={() => clearBinding(c.id)} disabled={!b}>✕</button>
              </div>
            );
          })}
        </div>

        {status && <div className="ctrl-status mono">{status}</div>}
      </div>
    </div>
  );
}
