//! Controller-profile storage: load/list/save `ControllerProfile` JSON files from the user's
//! controller directory (and, later, a bundled read-only dir). See `docs/CONTROLLER-ARCHITECTURE.md`.
//!
//! A profile is pure data (bindings + optional script) — adding a controller is dropping a file
//! here, no recompile. This module is plain blocking I/O, called only from Tauri commands.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};

use compas_core::{ControllerProfile, InputKind, Mapping, Registry};
use compas_script::ScriptRuntime;
use midir::{MidiOutput, MidiOutputConnection};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Number of decks the control registry is built for (matches the engine).
const DECK_COUNT: usize = 4;
/// Soft-takeover pickup threshold in normalized units (~4 MIDI steps).
const TAKEOVER: f64 = 0.03;

/// The user controller directory (`<app-data>/controllers`), created if missing.
pub fn profiles_dir(base: &Path) -> std::io::Result<PathBuf> {
    let dir = base.join("controllers");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Load and parse every `*.json` profile in `dir`, skipping any that fail to parse.
pub fn list_profiles(dir: &Path) -> Vec<ControllerProfile> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(p) = load_profile(&path) {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// List profiles from a bundled (read-only) dir and the user dir, merged by id — a user profile
/// overrides a bundled one with the same id. Sorted by name.
pub fn list_merged(bundled: Option<&Path>, user: &Path) -> Vec<ControllerProfile> {
    use std::collections::BTreeMap;
    let mut by_id: BTreeMap<String, ControllerProfile> = BTreeMap::new();
    if let Some(b) = bundled {
        for p in list_profiles(b) {
            by_id.insert(p.id.clone(), p);
        }
    }
    for p in list_profiles(user) {
        by_id.insert(p.id.clone(), p); // user overrides bundled
    }
    let mut out: Vec<_> = by_id.into_values().collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Load a single profile file.
pub fn load_profile(path: &Path) -> Result<ControllerProfile, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Save (or overwrite) a profile as `<dir>/<id>.json`. Used by the guided learn editor.
pub fn save_profile(dir: &Path, profile: &ControllerProfile) -> Result<PathBuf, String> {
    let safe: String = profile
        .id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let id = if safe.is_empty() { "profile".to_string() } else { safe };
    let path = dir.join(format!("{id}.json"));
    let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A resolved control change pushed to the frontend, which applies it through the deck/mixer setters.
#[derive(Serialize, Clone)]
pub struct ControllerUpdateEvent {
    /// Control-bus id, e.g. `"deck.0.gain"`.
    pub control: String,
    /// Engine-domain value (the unit the matching UI setter expects).
    pub value: f64,
}

/// Messages to the controller engine thread.
pub enum ControllerMsg {
    /// A raw MIDI message (status, data1, data2) forwarded from the input port.
    Midi(u8, u8, u8),
    /// Activate a profile (declarative bindings + optional script).
    Activate(Box<ControllerProfile>),
    /// Drop the active profile.
    Deactivate,
    /// Open (or close, with `None`) a MIDI output port for LED/feedback echo — matched by name
    /// substring against the OS output ports.
    SetOutputPort(Option<String>),
    /// Reflect a control's current engine value back onto the device (LED rings, motor faders).
    /// Driven by the frontend whenever a control changes — from the UI *or* the controller — so the
    /// hardware tracks software state, not just controller-originated moves.
    Feedback { control: String, value: f64 },
}

/// Handle to the controller engine thread. The thread owns the (`!Send`) script runtime, so all it
/// exposes is a message sender.
pub struct ControllerEngine {
    tx: Sender<ControllerMsg>,
}

impl ControllerEngine {
    /// Spawn the engine thread. It owns the script runtime + active mapping and emits
    /// `controller:update` events the frontend applies.
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel::<ControllerMsg>();
        std::thread::Builder::new()
            .name("compas-controller".into())
            .spawn(move || run(app, rx))
            .expect("spawn controller thread");
        ControllerEngine { tx }
    }

    pub fn send(&self, msg: ControllerMsg) {
        let _ = self.tx.send(msg);
    }

    /// A cloneable sender for forwarding raw MIDI from the input callback.
    pub fn sender(&self) -> Sender<ControllerMsg> {
        self.tx.clone()
    }
}

/// Controller engine loop: resolve forwarded MIDI through the active profile (declarative bindings
/// first, then the script's `onMidi`), and emit the resulting control updates.
fn run(app: AppHandle, rx: mpsc::Receiver<ControllerMsg>) {
    let registry = Registry::defaults(DECK_COUNT);
    let mut mapping = Mapping::default();
    let mut script: Option<ScriptRuntime> = None;
    // Last normalized value applied per control, for soft-takeover.
    let mut current: HashMap<String, f64> = HashMap::new();
    // MIDI output for LED/feedback (controller echo + UI-driven changes).
    let mut out: Option<MidiOutputConnection> = None;
    // Last MIDI value sent per output address (status+data1), to skip redundant resends.
    let mut fb_last: HashMap<[u8; 2], u8> = HashMap::new();

    for msg in rx {
        match msg {
            ControllerMsg::SetOutputPort(name) => {
                out = name.and_then(|name| open_output(&name));
                fb_last.clear(); // a fresh port's LED/fader state is unknown
            }
            ControllerMsg::Feedback { control, value } => {
                if out.is_none() {
                    continue;
                }
                // Engine value → MIDI via the control's behavior, sent to every input bound to it.
                if let Some(spec) = registry.get_str(&control) {
                    let mv = spec.behavior.to_midi(value);
                    for b in &mapping.bindings {
                        if b.control.as_str() == control {
                            send_feedback(&mut out, &mut fb_last, b.channel, b.input, mv);
                        }
                    }
                }
            }
            ControllerMsg::Activate(profile) => {
                mapping = profile.mapping();
                script = profile.script.as_deref().and_then(|src| match ScriptRuntime::new() {
                    Ok(rt) => {
                        if let Err(e) = rt.eval(src) {
                            tracing::warn!("controller script error: {e}");
                        }
                        Some(rt)
                    }
                    Err(e) => {
                        tracing::warn!("controller script runtime: {e}");
                        None
                    }
                });
                current.clear();
                fb_last.clear(); // re-send feedback for the new mapping's addresses
            }
            ControllerMsg::Deactivate => {
                mapping = Mapping::default();
                script = None;
                current.clear();
                fb_last.clear();
            }
            ControllerMsg::Midi(status, d1, d2) => {
                let channel = status & 0x0F;
                let input = match status & 0xF0 {
                    0xB0 => Some(InputKind::Cc { cc: d1 }),
                    0x90 | 0x80 => Some(InputKind::Note { note: d1 }),
                    _ => None,
                };
                let mut updates: Vec<ControllerUpdateEvent> = Vec::new();

                if let Some(inp) = input {
                    if let Some(binding) = mapping.find(channel, inp) {
                        let (b_channel, b_input) = (binding.channel, binding.input);
                        // Soft-takeover uses the last value we applied; unknown → adopt immediately.
                        let cur = current
                            .get(binding.control.as_str())
                            .copied()
                            .unwrap_or(d2 as f64 / 127.0);
                        if let Some(u) = mapping.resolve(&registry, channel, inp, d2, cur, TAKEOVER) {
                            // Echo back to the device on the same address (immediate LED/feedback for
                            // the control the user just moved; UI changes flow via Feedback).
                            let mv = (u.normalized * 127.0).round().clamp(0.0, 127.0) as u8;
                            send_feedback(&mut out, &mut fb_last, b_channel, b_input, mv);
                            current.insert(u.control.as_str().to_string(), u.normalized);
                            updates.push(ControllerUpdateEvent {
                                control: u.control.as_str().to_string(),
                                value: u.value,
                            });
                        }
                    } else if let Some(rt) = script.as_ref() {
                        // No declarative binding — let the script handle it.
                        if let Ok(ups) = rt.on_midi(status, d1, d2) {
                            for cu in ups {
                                // Script `engine.set` values are normalized by convention.
                                let norm = cu.value.clamp(0.0, 1.0);
                                let value = registry
                                    .get_str(&cu.control)
                                    .map(|s| s.behavior.from_normalized(norm))
                                    .unwrap_or(norm);
                                current.insert(cu.control.clone(), norm);
                                updates.push(ControllerUpdateEvent { control: cu.control, value });
                            }
                        }
                    }
                }

                for u in updates {
                    let _ = app.emit("controller:update", u);
                }
            }
        }
    }
}

/// Open a MIDI output port whose name contains `name` (for LED/feedback echo).
fn open_output(name: &str) -> Option<MidiOutputConnection> {
    let midi = MidiOutput::new("compas-out").ok()?;
    let ports = midi.ports();
    let port = ports
        .iter()
        .find(|p| midi.port_name(p).map(|n| n.contains(name)).unwrap_or(false))?;
    midi.connect(port, "compas-feedback").ok()
}

/// Send a feedback value to a control's input address on the device, skipping the send when the
/// address already holds that value (avoids flooding LEDs / motor faders with redundant messages).
/// No-op when no output port is open.
fn send_feedback(
    out: &mut Option<MidiOutputConnection>,
    last: &mut HashMap<[u8; 2], u8>,
    channel: u8,
    input: InputKind,
    value: u8,
) {
    let Some(conn) = out.as_mut() else { return };
    let bytes = midi_bytes(channel, input, value);
    let addr = [bytes[0], bytes[1]]; // status (type+channel) + note/cc number
    if feedback_changed(last, addr, value) {
        let _ = conn.send(&bytes);
    }
}

/// Record `value` for an output address, returning whether it differs from the last value sent
/// there (so callers skip redundant device writes). Pure so it can be unit-tested without a port.
fn feedback_changed(last: &mut HashMap<[u8; 2], u8>, addr: [u8; 2], value: u8) -> bool {
    if last.get(&addr) == Some(&value) {
        return false;
    }
    last.insert(addr, value);
    true
}

/// Build a 3-byte MIDI message echoing `value` (0..127) to a binding's input address.
fn midi_bytes(channel: u8, input: InputKind, value: u8) -> [u8; 3] {
    let ch = channel & 0x0F;
    match input {
        InputKind::Cc { cc } => [0xB0 | ch, cc, value],
        InputKind::Note { note } => [0x90 | ch, note, value],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_dedups_per_address() {
        let mut last = HashMap::new();
        let a = [0xB0, 7]; // CC 7 on ch 0
        let b = [0x90, 36]; // note 36 on ch 0
        assert!(feedback_changed(&mut last, a, 100)); // first value sends
        assert!(!feedback_changed(&mut last, a, 100)); // same value is skipped
        assert!(feedback_changed(&mut last, a, 101)); // a change sends
        assert!(feedback_changed(&mut last, b, 100)); // a different address is independent
        assert!(!feedback_changed(&mut last, b, 100));
    }

    #[test]
    fn midi_bytes_builds_cc_and_note() {
        assert_eq!(midi_bytes(0, InputKind::Cc { cc: 7 }, 100), [0xB0, 7, 100]);
        assert_eq!(midi_bytes(2, InputKind::Note { note: 36 }, 127), [0x92, 36, 127]);
    }

    #[test]
    fn bundled_profiles_parse_and_target_real_controls() {
        // Every binding in every shipped profile must resolve to a control the engine exposes —
        // a typo'd id silently no-ops at runtime, so catch it here. Also guards that the files are
        // valid JSON / `ControllerProfile`.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/controllers");
        let profiles = list_profiles(&dir);
        assert!(
            profiles.len() >= 3,
            "expected the bundled starter profiles, found {}",
            profiles.len()
        );
        let registry = Registry::defaults(DECK_COUNT);
        for p in &profiles {
            assert!(!p.id.is_empty(), "profile is missing an id");
            assert!(!p.bindings.is_empty(), "{} has no bindings", p.id);
            for b in &p.bindings {
                assert!(
                    registry.get(&b.control).is_some(),
                    "{} binds unknown control {:?}",
                    p.id,
                    b.control.as_str()
                );
            }
        }
    }

    #[test]
    fn save_then_list_round_trips() {
        let tmp = std::env::temp_dir().join(format!("compas-ctrl-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let mut p = ControllerProfile {
            id: "vendor model v1!".into(), // sanitized to vendor-model-v1-
            name: "Vendor Model".into(),
            ..Default::default()
        };
        p.bindings.push(compas_core::Binding {
            channel: 0,
            input: compas_core::InputKind::Cc { cc: 7 },
            control: "deck.0.gain".into(),
            soft_takeover: false,
        });

        let path = save_profile(&tmp, &p).unwrap();
        assert!(path.exists());
        let listed = list_profiles(&tmp);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Vendor Model");
        assert_eq!(listed[0].bindings.len(), 1);

        // A malformed file is skipped, not fatal.
        fs::write(tmp.join("broken.json"), "{ not json").unwrap();
        assert_eq!(list_profiles(&tmp).len(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }
}
