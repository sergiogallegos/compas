//! Sampler / performance pads: a small bank of one-shot (or looped) PCM samples summed into
//! the master bus, played from a fixed voice pool. RT-safe — `process()`/`trigger()` never
//! allocate. Sample buffers are decoded on the control thread and installed via a command;
//! the replaced `Arc<DeckBuffer>` is handed back for reclaim so nothing frees on the audio
//! thread. Conceptually a sibling of [`Synth`](compas_dsp::Synth), but it reads PCM rather
//! than synthesising oscillators, so it lives here next to [`DeckBuffer`].

use std::sync::Arc;

use compas_core::DeckBuffer;
use compas_dsp::GainSmoother;

/// Number of pads in the sampler bank.
pub const NUM_PADS: usize = 8;
/// Simultaneous sample voices (retriggers/overlaps past this steal the oldest).
const SAMPLER_VOICES: usize = 16;

#[derive(Clone, Copy)]
struct SampleVoice {
    /// Which pad slot this voice is playing.
    slot: usize,
    /// Fractional read position in source frames.
    playhead: f64,
    /// source_rate / device_rate — keeps pitch correct across rate mismatch.
    base_ratio: f64,
    gain: f32,
    looping: bool,
    active: bool,
}

impl SampleVoice {
    fn idle() -> Self {
        SampleVoice {
            slot: 0,
            playhead: 0.0,
            base_ratio: 1.0,
            gain: 0.0,
            looping: false,
            active: false,
        }
    }
}

pub struct Sampler {
    slots: [Option<Arc<DeckBuffer>>; NUM_PADS],
    /// Per-pad loop mode: a looped pad toggles play/stop on trigger; a one-shot pad overlaps.
    looped: [bool; NUM_PADS],
    voices: [SampleVoice; SAMPLER_VOICES],
    device_rate: f32,
    gain: GainSmoother,
    /// Round-robin cursor for voice stealing when all voices are busy.
    next_voice: usize,
}

impl Sampler {
    pub fn new(device_rate: f32) -> Self {
        Sampler {
            slots: std::array::from_fn(|_| None),
            looped: [false; NUM_PADS],
            voices: [SampleVoice::idle(); SAMPLER_VOICES],
            device_rate,
            gain: GainSmoother::new(0.9, device_rate, 10.0),
            next_voice: 0,
        }
    }

    /// Install (or clear, with `None`) a pad's sample. Stops any voices on that slot and
    /// returns the previous buffer so the caller can reclaim it off the audio thread.
    pub fn set_slot(&mut self, slot: usize, buffer: Option<Arc<DeckBuffer>>) -> Option<Arc<DeckBuffer>> {
        if slot >= NUM_PADS {
            return None;
        }
        for v in self.voices.iter_mut() {
            if v.active && v.slot == slot {
                v.active = false;
            }
        }
        std::mem::replace(&mut self.slots[slot], buffer)
    }

    pub fn set_loop(&mut self, slot: usize, looping: bool) {
        if let Some(l) = self.looped.get_mut(slot) {
            *l = looping;
        }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain.set_target(gain.max(0.0));
    }

    /// Trigger a pad. One-shot pads start a fresh (overlapping) voice each press; looped pads
    /// toggle: the first press starts the loop, the next stops it. RT-SAFE.
    pub fn trigger(&mut self, slot: usize, velocity: u8) {
        let Some(buf) = self.slots.get(slot).and_then(|s| s.as_ref()) else {
            return;
        };
        if buf.frames() == 0 {
            return;
        }
        let looping = self.looped.get(slot).copied().unwrap_or(false);
        if looping {
            // Toggle: if this loop is already playing, stop it instead of stacking another.
            if let Some(v) = self
                .voices
                .iter_mut()
                .find(|v| v.active && v.slot == slot && v.looping)
            {
                v.active = false;
                return;
            }
        }
        let base_ratio = buf.source_rate as f64 / self.device_rate as f64;
        let i = self.alloc_voice();
        self.voices[i] = SampleVoice {
            slot,
            playhead: 0.0,
            base_ratio,
            gain: (velocity as f32 / 127.0).clamp(0.0, 1.0),
            looping,
            active: true,
        };
    }

    /// Stop all voices playing `slot` (e.g. a manual stop or pad cleared).
    pub fn stop(&mut self, slot: usize) {
        for v in self.voices.iter_mut() {
            if v.slot == slot {
                v.active = false;
            }
        }
    }

    /// Pick a free voice, else steal the next round-robin slot.
    fn alloc_voice(&mut self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| !v.active) {
            return i;
        }
        let i = self.next_voice;
        self.next_voice = (self.next_voice + 1) % SAMPLER_VOICES;
        i
    }

    /// Sum all active voices into one stereo frame at the global level. RT-SAFE.
    pub fn process(&mut self) -> (f32, f32) {
        let g = self.gain.next_gain();
        let mut l = 0.0;
        let mut r = 0.0;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            let Some(buf) = self.slots[v.slot].as_ref() else {
                v.active = false;
                continue;
            };
            let frames = buf.frames();
            if frames < 2 {
                v.active = false;
                continue;
            }
            if v.playhead >= (frames - 1) as f64 {
                if v.looping {
                    v.playhead = 0.0;
                } else {
                    v.active = false;
                    continue;
                }
            }
            let (sl, sr) = interp(&buf.samples, frames, v.playhead);
            l += sl * v.gain;
            r += sr * v.gain;
            v.playhead += v.base_ratio;
        }
        (l * g, r * g)
    }
}

/// Linear interpolation of an interleaved-stereo buffer at a fractional frame position.
/// Plenty for one-shot samples (decks use cubic for varispeed/scratch where it matters more).
#[inline]
fn interp(samples: &[f32], frames: usize, pos: f64) -> (f32, f32) {
    let i = pos.floor() as usize;
    let frac = (pos - i as f64) as f32;
    let j = (i + 1).min(frames - 1);
    let l = samples[i * 2] + (samples[j * 2] - samples[i * 2]) * frac;
    let r = samples[i * 2 + 1] + (samples[j * 2 + 1] - samples[i * 2 + 1]) * frac;
    (l, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(val: f32, frames: usize) -> Arc<DeckBuffer> {
        Arc::new(DeckBuffer::new(vec![val; frames * 2], 48_000))
    }

    #[test]
    fn empty_pad_is_silent_and_trigger_is_a_noop() {
        let mut s = Sampler::new(48_000.0);
        s.trigger(0, 127);
        let (l, r) = s.process();
        assert_eq!((l, r), (0.0, 0.0));
    }

    #[test]
    fn one_shot_plays_then_stops() {
        let mut s = Sampler::new(48_000.0);
        s.set_slot(0, Some(buf(0.5, 4)));
        s.gain = GainSmoother::new(1.0, 48_000.0, 10.0);
        s.trigger(0, 127);
        let mut heard = false;
        for _ in 0..8 {
            if s.process().0.abs() > 0.0 {
                heard = true;
            }
        }
        assert!(heard, "a triggered one-shot should produce audio");
        // After the 4-frame sample ends, the voice frees and output returns to silence.
        let (l, _r) = s.process();
        assert_eq!(l, 0.0);
    }

    #[test]
    fn looped_pad_toggles_on_and_off() {
        let mut s = Sampler::new(48_000.0);
        s.set_slot(0, Some(buf(0.4, 4)));
        s.gain = GainSmoother::new(1.0, 48_000.0, 10.0);
        s.set_loop(0, true);
        s.trigger(0, 127); // start
        for _ in 0..20 {
            s.process();
        }
        assert!(s.process().0.abs() > 0.0, "a looped pad keeps sounding");
        s.trigger(0, 127); // toggle off
        let (l, _r) = s.process();
        assert_eq!(l, 0.0, "second trigger stops the loop");
    }

    #[test]
    fn clearing_a_slot_returns_old_buffer_and_stops_voices() {
        let mut s = Sampler::new(48_000.0);
        s.set_slot(0, Some(buf(0.5, 100)));
        s.trigger(0, 127);
        let old = s.set_slot(0, None);
        assert!(old.is_some(), "set_slot returns the replaced buffer for reclaim");
        assert_eq!(s.process().0, 0.0, "clearing the pad silences its voice");
    }
}
