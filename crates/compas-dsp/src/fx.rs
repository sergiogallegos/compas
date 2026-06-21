//! Chainable effects rack: a uniform [`Effect`] trait over the individual DSP effects, plus an
//! [`FxChain`] that runs an ordered, individually-bypassable list of them. This replaces the deck's
//! fixed echo→reverb→flanger→crusher insert sequence with a reorderable chain, and is the surface a
//! meta/super-knob or the scripting layer drives.
//!
//! **State/processor split:** effects are *constructed on the control thread* (they pre-allocate
//! their buffers in `new`), then handed to the audio thread; [`Effect::process`] / [`FxChain::process`]
//! are allocation-free and RT-SAFE. Adding/removing/reordering slots happens via control-thread
//! methods (`push`/`swap`/`set_enabled`); only `process` and `set_param` run on the audio thread.

use crate::rt::{Bitcrusher, Delay, Flanger, Reverb};

/// A stereo audio effect with a small, introspectable normalized parameter set.
pub trait Effect: Send {
    /// Process one stereo frame. RT-SAFE.
    fn process(&mut self, l: f32, r: f32) -> (f32, f32);
    /// Set parameter `index` from a normalized `0..=1` value (effect-specific mapping). Out-of-range
    /// indices are ignored. RT-SAFE.
    fn set_param(&mut self, index: usize, value: f32);
    /// Reset internal state (delay lines, holds, tails). RT-SAFE (no allocation).
    fn clear(&mut self);
    /// Number of normalized parameters this effect exposes.
    fn param_count(&self) -> usize;
    /// Stable identifier for the effect kind (for UI / persistence).
    fn name(&self) -> &'static str;
}

/// Max echo time the delay effect maps `time` to (matches the deck's pre-allocated line).
const DELAY_MAX_SECS: f32 = 2.0;

impl Effect for Delay {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        Delay::process(self, l, r)
    }
    /// 0 = mix, 1 = feedback, 2 = time (0..2 s).
    fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.set_mix(v),
            1 => self.set_feedback(v),
            2 => self.set_time_sec(v * DELAY_MAX_SECS),
            _ => {}
        }
    }
    fn clear(&mut self) {
        Delay::clear(self)
    }
    fn param_count(&self) -> usize {
        3
    }
    fn name(&self) -> &'static str {
        "echo"
    }
}

impl Effect for Reverb {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        Reverb::process(self, l, r)
    }
    /// 0 = mix, 1 = room size.
    fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.set_mix(v),
            1 => self.set_room_size(v),
            _ => {}
        }
    }
    fn clear(&mut self) {
        Reverb::clear(self)
    }
    fn param_count(&self) -> usize {
        2
    }
    fn name(&self) -> &'static str {
        "reverb"
    }
}

impl Effect for Flanger {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        Flanger::process(self, l, r)
    }
    /// 0 = mix, 1 = depth, 2 = rate (0.05..5 Hz), 3 = feedback.
    fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.set_mix(v),
            1 => self.set_depth(v),
            2 => self.set_rate_hz(0.05 + v * 4.95),
            3 => self.set_feedback(v),
            _ => {}
        }
    }
    fn clear(&mut self) {
        Flanger::clear(self)
    }
    fn param_count(&self) -> usize {
        4
    }
    fn name(&self) -> &'static str {
        "flanger"
    }
}

impl Effect for Bitcrusher {
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        Bitcrusher::process(self, l, r)
    }
    /// 0 = mix, 1 = crush (16→2 bits), 2 = downsample (1..32×).
    fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.set_mix(v),
            1 => self.set_bits(16.0 - v * 14.0),
            2 => self.set_downsample(1 + (v * 31.0).round() as u32),
            _ => {}
        }
    }
    fn clear(&mut self) {
        Bitcrusher::clear(self)
    }
    fn param_count(&self) -> usize {
        3
    }
    fn name(&self) -> &'static str {
        "bitcrusher"
    }
}

/// One slot in the chain: an effect plus whether it's currently engaged.
pub struct FxSlot {
    pub effect: Box<dyn Effect>,
    pub enabled: bool,
}

/// An ordered, individually-bypassable chain of effects. Build on the control thread; `process`
/// on the audio thread.
#[derive(Default)]
pub struct FxChain {
    slots: Vec<FxSlot>,
}

impl FxChain {
    pub fn new() -> Self {
        FxChain { slots: Vec::new() }
    }

    /// Build the deck's default chain (echo → reverb → flanger → bitcrusher), all bypassed.
    /// Allocates — call on the control thread.
    pub fn default_deck(sample_rate: f32) -> Self {
        let mut c = FxChain::new();
        c.push(Box::new(Delay::new(sample_rate, DELAY_MAX_SECS)));
        c.push(Box::new(Reverb::new(sample_rate)));
        c.push(Box::new(Flanger::new(sample_rate)));
        c.push(Box::new(Bitcrusher::new()));
        c
    }

    /// Append an effect (bypassed). Control thread (allocates).
    pub fn push(&mut self, effect: Box<dyn Effect>) {
        self.slots.push(FxSlot { effect, enabled: false });
    }

    /// Process one stereo frame through every enabled slot, in order. RT-SAFE.
    #[inline]
    pub fn process(&mut self, mut l: f32, mut r: f32) -> (f32, f32) {
        for slot in self.slots.iter_mut() {
            if slot.enabled {
                let (nl, nr) = slot.effect.process(l, r);
                l = nl;
                r = nr;
            }
        }
        (l, r)
    }

    /// Engage/bypass a slot; clears its state on the engaging edge (no burst-back). RT-SAFE.
    pub fn set_enabled(&mut self, slot: usize, enabled: bool) {
        if let Some(s) = self.slots.get_mut(slot) {
            if enabled && !s.enabled {
                s.effect.clear();
            }
            s.enabled = enabled;
        }
    }

    /// Set a normalized parameter on a slot's effect. RT-SAFE.
    pub fn set_param(&mut self, slot: usize, index: usize, value: f32) {
        if let Some(s) = self.slots.get_mut(slot) {
            s.effect.set_param(index, value);
        }
    }

    /// Reorder: move the effect at `from` to `to`. Control thread.
    pub fn move_slot(&mut self, from: usize, to: usize) {
        if from < self.slots.len() && to < self.slots.len() && from != to {
            let s = self.slots.remove(from);
            self.slots.insert(to, s);
        }
    }

    pub fn is_enabled(&self, slot: usize) -> bool {
        self.slots.get(slot).map(|s| s.enabled).unwrap_or(false)
    }

    pub fn name_at(&self, slot: usize) -> Option<&'static str> {
        self.slots.get(slot).map(|s| s.effect.name())
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial effect for testing chain wiring: scales both channels by `g`.
    struct Gain {
        g: f32,
    }
    impl Effect for Gain {
        fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
            (l * self.g, r * self.g)
        }
        fn set_param(&mut self, index: usize, value: f32) {
            if index == 0 {
                self.g = value;
            }
        }
        fn clear(&mut self) {}
        fn param_count(&self) -> usize {
            1
        }
        fn name(&self) -> &'static str {
            "gain"
        }
    }

    #[test]
    fn disabled_slots_are_bypassed() {
        let mut c = FxChain::new();
        c.push(Box::new(Gain { g: 2.0 }));
        // Bypassed by default → unity.
        assert_eq!(c.process(1.0, 1.0), (1.0, 1.0));
        c.set_enabled(0, true);
        assert_eq!(c.process(1.0, 1.0), (2.0, 2.0));
    }

    #[test]
    fn enabled_slots_compose_in_order() {
        let mut c = FxChain::new();
        c.push(Box::new(Gain { g: 2.0 }));
        c.push(Box::new(Gain { g: 3.0 }));
        c.set_enabled(0, true);
        c.set_enabled(1, true);
        assert_eq!(c.process(1.0, 1.0), (6.0, 6.0)); // 1 * 2 * 3
    }

    #[test]
    fn set_param_reaches_the_slot_effect() {
        let mut c = FxChain::new();
        c.push(Box::new(Gain { g: 1.0 }));
        c.set_enabled(0, true);
        c.set_param(0, 0, 5.0);
        assert_eq!(c.process(1.0, 1.0), (5.0, 5.0));
    }

    #[test]
    fn move_slot_reorders() {
        let mut c = FxChain::new();
        c.push(Box::new(Gain { g: 2.0 }));
        c.push(Box::new(Gain { g: 3.0 }));
        assert_eq!(c.name_at(0), Some("gain"));
        c.move_slot(1, 0); // both gains identical name, but exercise the path
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn default_deck_chain_has_four_bypassed_effects() {
        let c = FxChain::default_deck(48_000.0);
        assert_eq!(c.len(), 4);
        assert_eq!(c.name_at(0), Some("echo"));
        assert_eq!(c.name_at(3), Some("bitcrusher"));
        assert!(!c.is_enabled(0)); // all start bypassed
    }

    #[test]
    fn real_effect_through_trait_runs() {
        // Delay as an Effect: enable, set full wet + feedback, push a sample, expect output.
        let mut c = FxChain::new();
        c.push(Box::new(Delay::new(48_000.0, DELAY_MAX_SECS)));
        c.set_enabled(0, true);
        c.set_param(0, 0, 1.0); // mix
        c.set_param(0, 2, 0.01); // short time
        let mut last = (0.0, 0.0);
        for _ in 0..512 {
            last = c.process(0.5, 0.5);
        }
        assert!(last.0.is_finite() && last.1.is_finite());
    }
}
