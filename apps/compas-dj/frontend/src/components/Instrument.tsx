import { useEffect, useRef, useState } from "react";
import {
  allNotesOff,
  noteOff,
  noteOn,
  onMidiCc,
  setMidiSynth,
  setSynthGain,
  setSynthWaveform,
  type MidiCc,
} from "../lib/ipc";
import type { MidiApi } from "../hooks/useMidi";
import { Knob } from "./Knob";

const WAVES = ["SINE", "TRI", "SAW", "SQR"];
const KEY_COUNT = 25; // 2 octaves + 1, matching a 25-key controller
const BLACK = [1, 3, 6, 8, 10];

// QWERTY → semitone offset, so the synth is playable without any hardware.
const KEYMAP: Record<string, number> = {
  a: 0, w: 1, s: 2, e: 3, d: 4, f: 5, t: 6, g: 7, y: 8, h: 9, u: 10, j: 11,
  k: 12, o: 13, l: 14, p: 15, ";": 16,
};

const isBlack = (semitone: number) => BLACK.includes(((semitone % 12) + 12) % 12);

export function Instrument({ midi, onClose }: { midi: MidiApi; onClose: () => void }) {
  const [octave, setOctave] = useState(4); // C of this octave is the leftmost key
  const baseNote = octave * 12 + 12; // octave 4 → MIDI 60 (C4)
  const [wave, setWave] = useState(1);
  const [gain, setGain] = useState(0.6);
  const [cc, setCc] = useState<MidiCc | null>(null);
  const held = useRef<Set<number>>(new Set());

  const press = (note: number) => {
    if (held.current.has(note)) return;
    held.current.add(note);
    noteOn(note, 100).catch(() => {});
  };
  const release = (note: number) => {
    if (!held.current.has(note)) return;
    held.current.delete(note);
    noteOff(note).catch(() => {});
  };

  // Computer-keyboard playing.
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.repeat || e.metaKey || e.ctrlKey) return;
      const off = KEYMAP[e.key.toLowerCase()];
      if (off !== undefined) press(baseNote + off);
    };
    const up = (e: KeyboardEvent) => {
      const off = KEYMAP[e.key.toLowerCase()];
      if (off !== undefined) release(baseNote + off);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [baseNote]);

  // Route incoming MIDI notes to the synth while the instrument is open; show knob (CC)
  // feedback. Release everything (and stop synth routing) on unmount.
  useEffect(() => {
    setMidiSynth(true).catch(() => {});
    const un = onMidiCc(setCc);
    return () => {
      un.then((u) => u());
      setMidiSynth(false).catch(() => {});
      allNotesOff().catch(() => {});
    };
  }, []);

  const pickWave = (i: number) => {
    setWave(i);
    setSynthWaveform(i).catch(() => {});
  };
  const pickGain = (g: number) => {
    setGain(g);
    setSynthGain(g).catch(() => {});
  };

  const semis = Array.from({ length: KEY_COUNT }, (_, i) => i);
  const whites = semis.filter((s) => !isBlack(s));

  return (
    <div className="instrument">
      <div className="instrument-bar">
        <span className="overline">INSTRUMENT</span>
        <div className="inst-waves">
          {WAVES.map((w, i) => (
            <button key={w} className={`chip ${wave === i ? "chip--on" : ""}`} onClick={() => pickWave(i)}>{w}</button>
          ))}
        </div>
        <Knob value={gain} min={0} max={1.2} size={30} label="LEVEL" onChange={pickGain} />
        <div className="inst-oct">
          <button className="chip" onClick={() => setOctave((o) => Math.max(1, o - 1))}>OCT −</button>
          <span className="mono">C{octave}</span>
          <button className="chip" onClick={() => setOctave((o) => Math.min(7, o + 1))}>OCT +</button>
        </div>
        <div className="inst-midi">
          <select value={midi.portIdx} onChange={(e) => midi.setPortIdx(Number(e.target.value))} disabled={!!midi.connected}>
            {midi.ports.length ? midi.ports.map((p, i) => <option key={i} value={i}>{p}</option>) : <option>No MIDI devices</option>}
          </select>
          <button className={`chip ${midi.connected ? "chip--on" : ""}`} onClick={midi.toggle}>
            {midi.connected ? "MIDI ✓" : midi.ports.length ? "CONNECT" : "RESCAN"}
          </button>
          {cc && <span className="mono inst-cc">CC{cc.controller}:{cc.value}</span>}
        </div>
        <button className="chip inst-close" onClick={onClose} title="Close">✕</button>
      </div>
      <div className="piano">
        {whites.map((s) => (
          <button
            key={s}
            className="pkey pkey-w"
            onPointerDown={() => press(baseNote + s)}
            onPointerUp={() => release(baseNote + s)}
            onPointerLeave={() => release(baseNote + s)}
          />
        ))}
        {semis.filter(isBlack).map((s) => {
          const before = semis.filter((x) => x < s && !isBlack(x)).length;
          const w = 100 / whites.length;
          return (
            <button
              key={s}
              className="pkey pkey-b"
              style={{ left: `calc(${before * w}% - ${w * 0.3}%)`, width: `${w * 0.6}%` }}
              onPointerDown={() => press(baseNote + s)}
              onPointerUp={() => release(baseNote + s)}
              onPointerLeave={() => release(baseNote + s)}
            />
          );
        })}
      </div>
    </div>
  );
}
