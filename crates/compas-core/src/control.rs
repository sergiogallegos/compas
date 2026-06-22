//! Typed **control bus** — the shared vocabulary that connects the UI, MIDI controllers, and
//! (later) the scripting layer to engine parameters.
//!
//! The idea, in one sentence: every tweakable engine value is addressed by a stable string
//! [`ControlId`] (e.g. `"deck.0.gain"`) and carries a [`Behavior`] that maps cleanly between three
//! spaces — the **engine value** (domain units: a ratio, dB, Hz…), a **normalized** `0..=1` value
//! (what a fader or the UI speaks), and a **MIDI** `0..=127` value (what a controller speaks). One
//! engine value can then be driven correctly by a fader, a MIDI knob, the keyboard, or a script,
//! and reflected back to motorized faders / LED rings — all through the same conversions.
//!
//! This module is pure and `serde`-friendly so the [`Registry`] can be sent to the frontend, which
//! makes the UI introspectable (skinnable) and gives MIDI-learn / scripting a single source of
//! truth for what's mappable. It does **not** itself touch the audio thread; applying a normalized
//! value still flows through the engine's command protocol (the registry is the map, not the wire).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Stable identifier for a control, dotted lowest-to-highest scope, e.g. `"deck.0.eq.low"`,
/// `"mixer.crossfader"`, `"sampler.gain"`. Cheap to clone; `&'static str` for built-ins, owned
/// for anything constructed at runtime (scripts, custom mappings).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ControlId(pub Cow<'static, str>);

impl ControlId {
    /// Build from a `&'static str` with no allocation.
    pub const fn new(id: &'static str) -> Self {
        ControlId(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for ControlId {
    fn from(s: &'static str) -> Self {
        ControlId::new(s)
    }
}

impl From<String> for ControlId {
    fn from(s: String) -> Self {
        ControlId(Cow::Owned(s))
    }
}

impl std::fmt::Display for ControlId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Display unit for a control, used by the UI to label values. Not used in math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Dimensionless linear ratio (e.g. a gain of 1.0).
    Ratio,
    /// Decibels.
    Decibels,
    /// Hertz (filter cutoff…).
    Hertz,
    /// Percent (−8%..+8% tempo…).
    Percent,
    /// A count of beats (loop/FX time).
    Beats,
    /// On/off.
    Boolean,
    /// An index into a discrete set (waveform, mode…).
    Index,
    /// No natural unit.
    None,
}

/// How a control maps between engine value ⇄ normalized `0..=1` ⇄ MIDI `0..=127`.
///
/// All variants are total and round-trip stable (`from_normalized(to_normalized(v)) ≈ v` within the
/// representable range). Out-of-range inputs are clamped, never panicking — this runs near the audio
/// path and must be robust to junk from controllers/scripts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Behavior {
    /// Linear map `min..=max`. A centered control (EQ ±dB, tempo ±%, crossfader) is just a linear
    /// behavior whose center sits at normalized `0.5`.
    Linear { min: f64, max: f64 },
    /// Logarithmic (exponential) map `min..=max`, for perceptually-even frequency/gain sweeps.
    /// Requires `min > 0`.
    Logarithmic { min: f64, max: f64 },
    /// On/off: normalized `>= 0.5` (MIDI `>= 64`) is on (`1.0`), else off (`0.0`).
    Toggle,
    /// `steps` discrete values evenly spaced across `min..=max` inclusive (waveform index, beat-loop
    /// size…). `from_normalized` snaps to the nearest step.
    Stepped { min: f64, max: f64, steps: u32 },
}

impl Behavior {
    /// A centered/bipolar linear control, e.g. `centered(-8.0, 8.0)` for ±8 % tempo (center at 0).
    pub fn centered(extent_min: f64, extent_max: f64) -> Self {
        Behavior::Linear {
            min: extent_min,
            max: extent_max,
        }
    }

    /// Engine value for a normalized `0..=1` position (input clamped).
    pub fn from_normalized(&self, norm: f64) -> f64 {
        let n = norm.clamp(0.0, 1.0);
        match *self {
            Behavior::Linear { min, max } => min + (max - min) * n,
            Behavior::Logarithmic { min, max } => {
                let (lo, hi) = (min.max(f64::MIN_POSITIVE), max.max(f64::MIN_POSITIVE));
                lo * (hi / lo).powf(n)
            }
            Behavior::Toggle => {
                if n >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Behavior::Stepped { min, max, steps } => {
                if steps <= 1 {
                    return min;
                }
                let last = (steps - 1) as f64;
                let idx = (n * last).round();
                min + (max - min) * (idx / last)
            }
        }
    }

    /// Normalized `0..=1` position for an engine value (result clamped).
    pub fn to_normalized(&self, value: f64) -> f64 {
        let n = match *self {
            Behavior::Linear { min, max } => {
                if (max - min).abs() < f64::EPSILON {
                    0.0
                } else {
                    (value - min) / (max - min)
                }
            }
            Behavior::Logarithmic { min, max } => {
                let (lo, hi) = (min.max(f64::MIN_POSITIVE), max.max(f64::MIN_POSITIVE));
                let v = value.max(f64::MIN_POSITIVE);
                (v / lo).ln() / (hi / lo).ln()
            }
            Behavior::Toggle => {
                if value >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Behavior::Stepped { min, max, steps } => {
                if (max - min).abs() < f64::EPSILON || steps <= 1 {
                    0.0
                } else {
                    (value - min) / (max - min)
                }
            }
        };
        n.clamp(0.0, 1.0)
    }

    /// Engine value for a MIDI `0..=127` input.
    pub fn from_midi(&self, midi: u8) -> f64 {
        self.from_normalized(midi.min(127) as f64 / 127.0)
    }

    /// MIDI `0..=127` for an engine value — for LED rings / motor-fader feedback.
    pub fn to_midi(&self, value: f64) -> u8 {
        (self.to_normalized(value) * 127.0)
            .round()
            .clamp(0.0, 127.0) as u8
    }
}

/// Full description of one control: its id, how it maps, a human label, and a display unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlSpec {
    pub id: ControlId,
    pub behavior: Behavior,
    pub label: Cow<'static, str>,
    pub unit: Unit,
}

impl ControlSpec {
    pub fn new(
        id: impl Into<ControlId>,
        behavior: Behavior,
        label: impl Into<Cow<'static, str>>,
        unit: Unit,
    ) -> Self {
        ControlSpec {
            id: id.into(),
            behavior,
            label: label.into(),
            unit,
        }
    }
}

/// Soft-takeover gate: returns `true` only when a physical control's position is close enough to the
/// current software value to "pick it up" without a jump. Callers ignore incoming values until this
/// is true (e.g. after switching decks/layers, a knob that's physically elsewhere won't snap the
/// value). `threshold` is in normalized units (≈ `3.0/127.0` ≈ a few MIDI steps is typical).
pub fn soft_takeover_ok(software_norm: f64, hardware_norm: f64, threshold: f64) -> bool {
    (software_norm - hardware_norm).abs() <= threshold.max(0.0)
}

/// The set of controls compas exposes, keyed by [`ControlId`]. Built once (control thread) and
/// shared read-only; the frontend gets a serialized copy for the mapper/skin, and MIDI-learn /
/// scripts resolve targets through it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    specs: Vec<ControlSpec>,
}

impl Registry {
    pub fn new() -> Self {
        Registry { specs: Vec::new() }
    }

    /// Build the default registry for `deck_count` decks. This is the canonical list of what is
    /// mappable/scriptable; new engine parameters should be registered here.
    pub fn defaults(deck_count: usize) -> Self {
        let mut r = Registry::new();
        // Master / mixer.
        r.add(ControlSpec::new(
            "mixer.crossfader",
            Behavior::Linear { min: 0.0, max: 1.0 },
            "Crossfader",
            Unit::None,
        ));
        r.add(ControlSpec::new(
            "mixer.master_gain",
            Behavior::Linear { min: 0.0, max: 1.5 },
            "Master",
            Unit::Ratio,
        ));
        r.add(ControlSpec::new(
            "mixer.cue_mix",
            Behavior::Linear { min: 0.0, max: 1.0 },
            "Cue/Master",
            Unit::None,
        ));
        r.add(ControlSpec::new(
            "mixer.cue_volume",
            Behavior::Linear { min: 0.0, max: 1.0 },
            "Phones",
            Unit::Ratio,
        ));
        r.add(ControlSpec::new(
            "sampler.gain",
            Behavior::Linear { min: 0.0, max: 1.5 },
            "Sampler",
            Unit::Ratio,
        ));
        for pad in 0..8 {
            r.add(ControlSpec::new(
                ControlId(Cow::Owned(format!("sampler.{pad}.trigger"))),
                Behavior::Toggle,
                "Sampler Pad",
                Unit::Boolean,
            ));
        }
        // Per-deck controls.
        for d in 0..deck_count {
            let p = |suffix: &str| ControlId(Cow::Owned(format!("deck.{d}.{suffix}")));
            r.add(ControlSpec::new(
                p("gain"),
                Behavior::Linear { min: 0.0, max: 1.5 },
                "Gain",
                Unit::Ratio,
            ));
            for band in ["low", "mid", "high"] {
                r.add(ControlSpec::new(
                    ControlId(Cow::Owned(format!("deck.{d}.eq.{band}"))),
                    // ±26 dB full-kill range, centered at 0 dB (normalized 0.5).
                    Behavior::centered(-26.0, 26.0),
                    "EQ",
                    Unit::Decibels,
                ));
            }
            r.add(ControlSpec::new(
                p("filter"),
                // Bipolar DJ filter knob: −1 = full LPF, 0 = off, +1 = full HPF.
                Behavior::centered(-1.0, 1.0),
                "Filter",
                Unit::None,
            ));
            r.add(ControlSpec::new(
                p("tempo"),
                // ±8 % pitch fader, centered.
                Behavior::centered(-8.0, 8.0),
                "Tempo",
                Unit::Percent,
            ));
            r.add(ControlSpec::new(
                p("play"),
                Behavior::Toggle,
                "Play",
                Unit::Boolean,
            ));
            r.add(ControlSpec::new(
                p("cue"),
                Behavior::Toggle,
                "Cue",
                Unit::Boolean,
            ));
            r.add(ControlSpec::new(
                p("sync"),
                Behavior::Toggle,
                "Sync",
                Unit::Boolean,
            ));
            r.add(ControlSpec::new(
                p("keylock"),
                Behavior::Toggle,
                "Key Lock",
                Unit::Boolean,
            ));
        }
        r
    }

    /// Register (or replace, by id) a control spec.
    pub fn add(&mut self, spec: ControlSpec) {
        if let Some(existing) = self.specs.iter_mut().find(|s| s.id == spec.id) {
            *existing = spec;
        } else {
            self.specs.push(spec);
        }
    }

    /// Look up a control by id.
    pub fn get(&self, id: &ControlId) -> Option<&ControlSpec> {
        self.specs.iter().find(|s| &s.id == id)
    }

    /// Look up by raw string id.
    pub fn get_str(&self, id: &str) -> Option<&ControlSpec> {
        self.specs.iter().find(|s| s.id.as_str() == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ControlSpec> {
        self.specs.iter()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn linear_round_trips_and_hits_endpoints() {
        let b = Behavior::Linear { min: 0.0, max: 1.5 };
        assert!(approx(b.from_normalized(0.0), 0.0));
        assert!(approx(b.from_normalized(1.0), 1.5));
        assert!(approx(b.to_normalized(0.75), 0.5));
        for &v in &[0.0, 0.3, 0.75, 1.5] {
            assert!(approx(b.from_normalized(b.to_normalized(v)), v));
        }
    }

    #[test]
    fn centered_control_has_center_at_half() {
        let b = Behavior::centered(-8.0, 8.0);
        assert!(approx(b.from_normalized(0.5), 0.0));
        assert!(approx(b.to_normalized(0.0), 0.5));
        assert!(approx(b.from_normalized(0.0), -8.0));
        assert!(approx(b.from_normalized(1.0), 8.0));
    }

    #[test]
    fn logarithmic_is_monotonic_and_round_trips() {
        let b = Behavior::Logarithmic {
            min: 20.0,
            max: 20_000.0,
        };
        assert!(approx(b.from_normalized(0.0), 20.0));
        assert!(approx(b.from_normalized(1.0), 20_000.0));
        // Halfway up a log sweep is the geometric mean, not the arithmetic one.
        let mid = b.from_normalized(0.5);
        assert!((mid - (20.0_f64 * 20_000.0).sqrt()).abs() < 1e-6);
        for &v in &[20.0, 100.0, 1000.0, 20_000.0] {
            assert!((b.from_normalized(b.to_normalized(v)) - v).abs() < 1e-3);
        }
    }

    #[test]
    fn toggle_thresholds_at_half() {
        let b = Behavior::Toggle;
        assert_eq!(b.from_normalized(0.49), 0.0);
        assert_eq!(b.from_normalized(0.5), 1.0);
        assert_eq!(b.from_midi(63), 0.0);
        assert_eq!(b.from_midi(64), 1.0);
    }

    #[test]
    fn stepped_snaps_to_nearest_step() {
        // 4 waveform choices -> indices 0,1,2,3.
        let b = Behavior::Stepped {
            min: 0.0,
            max: 3.0,
            steps: 4,
        };
        assert!(approx(b.from_normalized(0.0), 0.0));
        assert!(approx(b.from_normalized(1.0), 3.0));
        assert!(approx(b.from_normalized(0.32), 1.0)); // ~0.33 -> step 1
        assert!(approx(b.from_normalized(0.7), 2.0));
    }

    #[test]
    fn midi_maps_endpoints() {
        let b = Behavior::Linear { min: 0.0, max: 1.0 };
        assert!(approx(b.from_midi(0), 0.0));
        assert!(approx(b.from_midi(127), 1.0));
        assert_eq!(b.to_midi(0.0), 0);
        assert_eq!(b.to_midi(1.0), 127);
    }

    #[test]
    fn out_of_range_inputs_clamp_not_panic() {
        let b = Behavior::Linear { min: 0.0, max: 1.0 };
        assert!(approx(b.from_normalized(-5.0), 0.0));
        assert!(approx(b.from_normalized(9.0), 1.0));
        assert!(approx(b.to_normalized(-100.0), 0.0));
        assert!(approx(b.to_normalized(100.0), 1.0));
        assert_eq!(b.to_midi(100.0), 127);
    }

    #[test]
    fn soft_takeover_only_within_threshold() {
        assert!(soft_takeover_ok(0.50, 0.51, 0.03));
        assert!(!soft_takeover_ok(0.20, 0.80, 0.03));
        assert!(soft_takeover_ok(0.20, 0.20, 0.0));
    }

    #[test]
    fn registry_exposes_sampler_pad_targets() {
        let r = Registry::defaults(4);
        assert!(r.get_str("sampler.gain").is_some());
        for pad in 0..8 {
            assert!(
                r.get_str(&format!("sampler.{pad}.trigger")).is_some(),
                "sampler pad {pad} should be mappable"
            );
        }
    }

    #[test]
    fn registry_defaults_cover_decks_and_mixer() {
        let r = Registry::defaults(4);
        assert!(r.get_str("mixer.crossfader").is_some());
        assert!(r.get_str("mixer.master_gain").is_some());
        assert!(r.get_str("deck.0.gain").is_some());
        assert!(r.get_str("deck.3.eq.high").is_some());
        assert!(r.get_str("deck.2.keylock").is_some());
        // No deck 4 when only 4 decks (0..=3).
        assert!(r.get_str("deck.4.gain").is_none());
        // Tempo is a centered percent control.
        let tempo = r.get_str("deck.1.tempo").unwrap();
        assert_eq!(tempo.unit, Unit::Percent);
        assert!(approx(tempo.behavior.from_normalized(0.5), 0.0));
    }

    #[test]
    fn registry_add_replaces_by_id() {
        let mut r = Registry::new();
        r.add(ControlSpec::new(
            "x",
            Behavior::Toggle,
            "first",
            Unit::Boolean,
        ));
        r.add(ControlSpec::new(
            "x",
            Behavior::Linear { min: 0.0, max: 1.0 },
            "second",
            Unit::Ratio,
        ));
        assert_eq!(r.len(), 1);
        assert_eq!(r.get_str("x").unwrap().label, "second");
    }
}
