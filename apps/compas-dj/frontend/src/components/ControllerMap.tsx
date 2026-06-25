import { useEffect, useMemo, useRef, useState } from "react";
import {
  controllerActivate,
  controllerList,
  controllerRegistry,
  controllerSave,
  exportProfilePack,
  importProfilePack,
  hidConnect,
  hidDisconnect,
  hidList,
  onHidInput,
  onMidiCc,
  onMidiNote,
  type ControllerBinding,
  type ControllerProfile,
  type ControlSpec,
  type HidDeviceInfo,
} from "../lib/ipc";
import { useMidi } from "../hooks/useMidi";

const slug = (s: string) =>
  s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "controller";

const inputLabel = (b: ControllerBinding) =>
  b.input.kind === "cc"
    ? `CC ${b.input.cc}`
    : b.input.kind === "hid"
      ? `HID byte ${b.input.byte}`
      : `Note ${b.input.note}`;

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
  const [hidDevices, setHidDevices] = useState<HidDeviceInfo[]>([]);
  const [hidPath, setHidPath] = useState("");
  const [lastInput, setLastInput] = useState("");
  const midi = useMidi();

  const learningRef = useRef<string | null>(null);
  learningRef.current = learning;

  useEffect(() => {
    controllerRegistry().then(setRegistry).catch(() => {});
    controllerList().then(setProfiles).catch(() => {});
  }, []);

  // Capture the next MIDI or HID message into the control being learned.
  useEffect(() => {
    const capture = (input: ControllerBinding["input"], channel: number, label: string) => {
      setLastInput(label);
      const control = learningRef.current;
      if (!control) return;
      setBindings((b) => ({ ...b, [control]: { channel, input, control, soft_takeover: true } }));
      setLearning(null);
      setStatus(`Bound ${control}`);
    };
    const unCc = onMidiCc((e) =>
      capture({ kind: "cc", cc: e.controller }, e.channel, `MIDI ch${e.channel + 1} CC ${e.controller} = ${e.value}`),
    );
    const unNote = onMidiNote((e) => {
      if (e.on) capture({ kind: "note", note: e.note }, e.channel, `MIDI ch${e.channel + 1} Note ${e.note}`);
    });
    // HID reports carry no channel; bind on channel 0 by the report byte that moved.
    const unHid = onHidInput((i) => capture({ kind: "hid", byte: i.byte }, 0, `HID byte ${i.byte} = ${i.value}`));
    return () => {
      unCc.then((u) => u());
      unNote.then((u) => u());
      unHid.then((u) => u());
    };
  }, []);

  const refreshHid = () => hidList().then(setHidDevices).catch(() => {});
  const connectHid = async () => {
    if (!hidPath) return;
    try {
      await hidConnect(hidPath);
      const d = hidDevices.find((x) => x.path === hidPath);
      setStatus(`HID connected: ${d ? d.product || d.path : hidPath}`);
    } catch (e) {
      setStatus(`HID connect failed: ${e}`);
    }
  };
  const disconnectHid = async () => {
    await hidDisconnect().catch(() => {});
    setStatus("HID disconnected");
  };

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
      // Sync the device LEDs/faders to current software state now that bindings are live.
      window.dispatchEvent(new Event("controller:resync"));
      setStatus(`Activated "${name}" (${profile.bindings.length} bindings)`);
    } catch (e) {
      setStatus(`Activate failed: ${e}`);
    }
  };
  // Share device maps: export all profiles to a pack file, or import a pack into the user dir.
  const exportPack = async () => {
    try {
      const count = await exportProfilePack([]);
      if (count !== null) setStatus(`Exported ${count} profile(s)`);
    } catch (e) {
      setStatus(`Export failed: ${e}`);
    }
  };
  const importPack = async () => {
    try {
      const ids = await importProfilePack();
      if (ids) {
        setProfiles(await controllerList());
        setStatus(`Imported ${ids.length} profile(s)`);
      }
    } catch (e) {
      setStatus(`Import failed: ${e}`);
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
          <button className="chip" onClick={exportPack} disabled={!profiles.length} title="Export all profiles to a shareable pack">Export pack</button>
          <button className="chip" onClick={importPack} title="Import controller profiles from a pack">Import pack</button>
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

        <div className="ctrl-hid">
          <span className="overline">MIDI IN</span>
          <select value={midi.portIdx} onChange={(e) => midi.setPortIdx(Number(e.target.value))} disabled={!!midi.connected}>
            {midi.ports.length ? (
              midi.ports.map((p, i) => <option key={p} value={i}>{p}</option>)
            ) : (
              <option value={0}>No MIDI devices</option>
            )}
          </select>
          <button className={`chip ${midi.connected ? "chip--on" : ""}`} onClick={midi.toggle}>
            {midi.connected ? "Disconnect" : midi.ports.length ? "Connect" : "Rescan"}
          </button>
          {lastInput && <span className="mono ctrl-last">{lastInput}</span>}
        </div>

        <p className="ctrl-hint">
          Click <strong>Learn</strong> on a control, then move that knob/pad/fader on your controller.
          The device's own MIDI/HID messages define the mapping.
        </p>

        <div className="ctrl-hid">
          <span className="overline">HID DEVICE</span>
          <button className="chip" onClick={refreshHid} title="Scan for HID controllers">Scan</button>
          <select value={hidPath} onChange={(e) => setHidPath(e.target.value)}>
            <option value="">{hidDevices.length ? "Select a device…" : "Scan to list devices"}</option>
            {hidDevices.map((d) => (
              <option key={d.path} value={d.path}>
                {(d.product || d.manufacturer || "HID") +
                  ` (${d.vendor_id.toString(16).padStart(4, "0")}:${d.product_id
                    .toString(16)
                    .padStart(4, "0")})`}
              </option>
            ))}
          </select>
          <button className="chip" onClick={connectHid} disabled={!hidPath}>Connect</button>
          <button className="chip" onClick={disconnectHid}>Disconnect</button>
        </div>
        <p className="ctrl-hint">
          For non-MIDI controllers (HID), Scan and Connect above, then Learn + wiggle — bindings
          capture the report byte that moves. (Absolute knobs/faders; LED output is device-specific.)
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
