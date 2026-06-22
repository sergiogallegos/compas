//! Declarative controller **mapping** over the [control bus](crate::control) — the foundation the
//! scripting layer plugs into.
//!
//! A [`Mapping`] is a serializable set of [`Binding`]s, each tying a physical MIDI input (a note or
//! CC on a channel) to a named [`ControlId`]. Resolving an incoming message looks up the binding,
//! runs the value through the control's [`Behavior`](crate::control::Behavior) (MIDI → engine
//! value), and honors **soft-takeover** so a knob that's physically out of position doesn't snap the
//! value when you switch decks/layers. This is the same model a sandboxed JS runtime would expose as
//! `engine.*` — declarative bindings cover the common case; scripting (a future `rquickjs`/`boa`
//! sandbox) is for logic that bindings can't express.

use serde::{Deserialize, Serialize};

use crate::control::{soft_takeover_ok, ControlId, Registry};

/// A physical control input on a controller (MIDI note/CC, or a raw HID report byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputKind {
    /// A note message (pad/key), identified by note number.
    Note { note: u8 },
    /// A continuous controller (knob/fader), identified by CC number.
    Cc { cc: u8 },
    /// A byte position in a non-MIDI HID input report (knob/fader/jog), read as an absolute
    /// `0..=255` value. Targets continuous controls; bit-packed buttons are a follow-up.
    Hid { byte: u8 },
}

impl InputKind {
    /// Full-scale raw value for this input kind — MIDI is 7-bit (`127`), HID bytes are 8-bit (`255`).
    pub fn full_scale(&self) -> f64 {
        match self {
            InputKind::Note { .. } | InputKind::Cc { .. } => 127.0,
            InputKind::Hid { .. } => 255.0,
        }
    }
}

/// One physical-input → engine-control binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// MIDI channel (0–15).
    pub channel: u8,
    pub input: InputKind,
    /// Target control id (e.g. `"deck.0.gain"`).
    pub control: ControlId,
    /// When true, ignore input until it matches the control's current value (no jumps).
    #[serde(default)]
    pub soft_takeover: bool,
}

/// A named, serializable set of bindings — what gets saved/shared as a controller profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String,
    pub bindings: Vec<Binding>,
}

/// Port-name hints used to auto-connect a controller (substring match against the OS port list).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortHints {
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
}

/// A complete, shareable controller profile: identity, port hints, the binding set, and an optional
/// device-logic script (loaded into a `compas-script` runtime by the host). This is the on-disk
/// format — bundled with the app and/or dropped into the user controller dir; it deserializes into a
/// usable [`Mapping`] plus metadata. (See `docs/CONTROLLER-ARCHITECTURE.md`.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControllerProfile {
    /// Stable slug, e.g. `"vendor-model-v1"`.
    pub id: String,
    /// Human display name.
    pub name: String,
    #[serde(default)]
    pub ports: PortHints,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    /// Optional device-logic script source (JS for the sandboxed runtime), when bindings aren't enough.
    #[serde(default)]
    pub script: Option<String>,
}

impl ControllerProfile {
    /// The declarative binding set as a ready-to-use [`Mapping`].
    pub fn mapping(&self) -> Mapping {
        Mapping {
            name: self.name.clone(),
            bindings: self.bindings.clone(),
        }
    }
}

/// The result of resolving a MIDI message: which control to set and to what.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedUpdate {
    pub control: ControlId,
    /// The engine-domain value (e.g. a dB amount, a gain ratio).
    pub value: f64,
    /// The normalized `0..=1` value it came from.
    pub normalized: f64,
}

impl Mapping {
    /// Find the binding for a given channel + input, if any.
    pub fn find(&self, channel: u8, input: InputKind) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|b| b.channel == channel && b.input == input)
    }

    /// Resolve a raw MIDI message (`value` is 7-bit `0..=127`) into a control update via the
    /// registry's behavior. `current_norm` is the control's present normalized value, used for
    /// soft-takeover; returns `None` if no binding matches, the control is unknown, or soft-takeover
    /// is engaged and the input hasn't caught up to the current value yet.
    pub fn resolve(
        &self,
        registry: &Registry,
        channel: u8,
        input: InputKind,
        value: u8,
        current_norm: f64,
        takeover_threshold: f64,
    ) -> Option<ResolvedUpdate> {
        let binding = self.find(channel, input)?;
        let spec = registry.get(&binding.control)?;
        let normalized = (value as f64 / input.full_scale()).min(1.0);
        if binding.soft_takeover && !soft_takeover_ok(current_norm, normalized, takeover_threshold) {
            return None;
        }
        Some(ResolvedUpdate {
            control: binding.control.clone(),
            value: spec.behavior.from_normalized(normalized),
            normalized,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> Mapping {
        Mapping {
            name: "test".into(),
            bindings: vec![
                Binding {
                    channel: 0,
                    input: InputKind::Cc { cc: 7 },
                    control: ControlId::new("deck.0.gain"),
                    soft_takeover: false,
                },
                Binding {
                    channel: 0,
                    input: InputKind::Cc { cc: 10 },
                    control: ControlId::new("mixer.crossfader"),
                    soft_takeover: true,
                },
            ],
        }
    }

    #[test]
    fn resolves_cc_through_control_behavior() {
        let reg = Registry::defaults(4);
        let m = mapping();
        // deck.0.gain is Linear 0..1.5; full MIDI → 1.5.
        let u = m
            .resolve(&reg, 0, InputKind::Cc { cc: 7 }, 127, 0.0, 0.03)
            .unwrap();
        assert_eq!(u.control.as_str(), "deck.0.gain");
        assert!((u.value - 1.5).abs() < 1e-6);
        assert!((u.normalized - 1.0).abs() < 1e-6);
    }

    #[test]
    fn unmatched_input_or_unknown_control_is_none() {
        let reg = Registry::defaults(4);
        let m = mapping();
        assert!(m.resolve(&reg, 0, InputKind::Cc { cc: 99 }, 64, 0.0, 0.03).is_none());
        assert!(m.resolve(&reg, 5, InputKind::Cc { cc: 7 }, 64, 0.0, 0.03).is_none());
    }

    #[test]
    fn soft_takeover_blocks_until_within_threshold() {
        let reg = Registry::defaults(4);
        let m = mapping();
        // Crossfader currently at 0.0; incoming knob at MIDI 127 (norm 1.0) is far → blocked.
        assert!(m
            .resolve(&reg, 0, InputKind::Cc { cc: 10 }, 127, 0.0, 0.03)
            .is_none());
        // Knob near the current value → picked up.
        let u = m.resolve(&reg, 0, InputKind::Cc { cc: 10 }, 1, 0.0, 0.03);
        assert!(u.is_some());
    }

    #[test]
    fn controller_profile_round_trips_and_yields_a_mapping() {
        let json = r#"{
            "id": "vendor-model-v1",
            "name": "Vendor Model",
            "ports": { "input": "Model MIDI", "output": "Model MIDI" },
            "bindings": [
                { "channel": 0, "input": { "kind": "cc", "cc": 7 }, "control": "deck.0.gain", "soft_takeover": true }
            ]
        }"#;
        let p: ControllerProfile = serde_json::from_str(json).unwrap();
        assert_eq!(p.id, "vendor-model-v1");
        assert_eq!(p.ports.input.as_deref(), Some("Model MIDI"));
        assert!(p.script.is_none());
        let m = p.mapping();
        assert_eq!(m.bindings.len(), 1);
        // The profile's binding resolves through the registry like any mapping. (soft_takeover is
        // on, so the incoming value must already be near the current — pass 127 with current ~1.0.)
        let reg = Registry::defaults(4);
        let u = m.resolve(&reg, 0, InputKind::Cc { cc: 7 }, 127, 1.0, 0.03).unwrap();
        assert_eq!(u.control.as_str(), "deck.0.gain");
    }

    #[test]
    fn hid_byte_resolves_at_8bit_scale() {
        let reg = Registry::defaults(4);
        let m = Mapping {
            name: "hid".into(),
            bindings: vec![Binding {
                channel: 0,
                input: InputKind::Hid { byte: 3 },
                control: ControlId::new("deck.0.gain"),
                soft_takeover: false,
            }],
        };
        // deck.0.gain is Linear 0..1.5; a full HID byte (255) → full scale → 1.5.
        let u = m
            .resolve(&reg, 0, InputKind::Hid { byte: 3 }, 255, 0.0, 0.03)
            .unwrap();
        assert!((u.value - 1.5).abs() < 1e-6);
        assert!((u.normalized - 1.0).abs() < 1e-6);
        // Half scale (~127/255) → ~0.5 normalized (MIDI would read 127 as full).
        let half = m
            .resolve(&reg, 0, InputKind::Hid { byte: 3 }, 127, 0.0, 0.03)
            .unwrap();
        assert!((half.normalized - 127.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn hid_input_serde_round_trips() {
        let json = r#"{ "kind": "hid", "byte": 7 }"#;
        let k: InputKind = serde_json::from_str(json).unwrap();
        assert_eq!(k, InputKind::Hid { byte: 7 });
        assert_eq!(k.full_scale(), 255.0);
    }

    #[test]
    fn mapping_serde_round_trips() {
        let m = mapping();
        let json = serde_json::to_string(&m).unwrap();
        let back: Mapping = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bindings.len(), 2);
        assert_eq!(back.bindings[1].control.as_str(), "mixer.crossfader");
        assert!(back.bindings[1].soft_takeover);
        assert_eq!(back.bindings[0].input, InputKind::Cc { cc: 7 });
    }
}
