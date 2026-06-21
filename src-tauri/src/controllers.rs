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
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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

    for msg in rx {
        match msg {
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
            }
            ControllerMsg::Deactivate => {
                mapping = Mapping::default();
                script = None;
                current.clear();
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
                        // Soft-takeover uses the last value we applied; unknown → adopt immediately.
                        let cur = current
                            .get(binding.control.as_str())
                            .copied()
                            .unwrap_or(d2 as f64 / 127.0);
                        if let Some(u) = mapping.resolve(&registry, channel, inp, d2, cur, TAKEOVER) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
