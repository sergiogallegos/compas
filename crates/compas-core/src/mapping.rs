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

/// A physical control input on a MIDI controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputKind {
    /// A note message (pad/key), identified by note number.
    Note { note: u8 },
    /// A continuous controller (knob/fader), identified by CC number.
    Cc { cc: u8 },
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
        let normalized = value.min(127) as f64 / 127.0;
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
