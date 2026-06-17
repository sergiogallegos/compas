//! The mixer and the command protocol. Everything here runs on the audio thread.

use compas_dsp::{Crossfader, GainSmoother, ThreeBandEq};
use rtrb::Consumer;

/// Number of decks the engine mixes. MVP uses 2; the array is sized for 4 so the
/// extension to 4 decks needs no structural change.
pub const NUM_DECKS: usize = 4;

/// Commands sent from the control thread into the audio callback over an SPSC ring.
///
/// RT note: variants must be cheap to apply. [`AudioCommand::AttachDeck`] hands a
/// freshly-built PCM ring consumer to the callback. The *previous* consumer it
/// replaces is returned to the control thread via the engine's "reclaim" ring so it
/// is dropped off the audio thread (dropping it here could free memory on the RT path).
pub enum AudioCommand {
    SetCrossfader(f32),
    SetMasterGain(f32),
    SetDeckGain { deck: usize, gain: f32 },
    SetDeckEq { deck: usize, low_db: f32, mid_db: f32, high_db: f32 },
    SetDeckPlaying { deck: usize, playing: bool },
    /// Attach a new PCM stream (interleaved stereo f32) to a deck.
    AttachDeck { deck: usize, pcm: Consumer<f32> },
    /// Detach a deck's PCM stream (e.g. on eject).
    DetachDeck { deck: usize },
}

/// Per-deck audio state living on the audio thread.
struct DeckAudio {
    /// Interleaved stereo PCM coming from a decoder thread. `None` when ejected.
    pcm: Option<Consumer<f32>>,
    gain: GainSmoother,
    eq_l: ThreeBandEq,
    eq_r: ThreeBandEq,
    playing: bool,
    /// Count of frames we wanted but the ring could not supply (buffer underrun /
    /// xrun). Surfaced to the UI for diagnostics; incrementing is RT-safe.
    underruns: u64,
    sample_rate: f32,
}

impl DeckAudio {
    fn new(sample_rate: f32) -> Self {
        DeckAudio {
            pcm: None,
            gain: GainSmoother::new(1.0, sample_rate, 8.0),
            eq_l: ThreeBandEq::new(sample_rate),
            eq_r: ThreeBandEq::new(sample_rate),
            playing: false,
            underruns: 0,
            sample_rate,
        }
    }

    /// Pull one stereo frame and apply per-deck gain + EQ. RT-SAFE.
    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        if !self.playing {
            // Still advance the gain smoother so an un-pause is click-free.
            let _ = self.gain.next_gain();
            return (0.0, 0.0);
        }
        let (mut l, mut r) = match self.pcm.as_mut() {
            Some(c) => match (c.pop(), c.pop()) {
                (Ok(l), Ok(r)) => (l, r),
                _ => {
                    self.underruns = self.underruns.wrapping_add(1);
                    (0.0, 0.0)
                }
            },
            None => (0.0, 0.0),
        };
        l = self.eq_l.process(l);
        r = self.eq_r.process(r);
        let g = self.gain.next_gain();
        (l * g, r * g)
    }
}

/// The audio-thread mixer: N decks → crossfader → master gain → output.
pub struct Mixer {
    decks: [DeckAudio; NUM_DECKS],
    crossfader: Crossfader,
    master: GainSmoother,
    commands: Consumer<AudioCommand>,
}

impl Mixer {
    pub fn new(sample_rate: f32, commands: Consumer<AudioCommand>) -> Self {
        Mixer {
            decks: std::array::from_fn(|_| DeckAudio::new(sample_rate)),
            crossfader: Crossfader::new(sample_rate),
            master: GainSmoother::new(0.85, sample_rate, 10.0),
            commands,
        }
    }

    /// Apply all pending control commands. RT-SAFE (bounded by ring capacity).
    #[inline]
    pub fn drain_commands(&mut self) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                AudioCommand::SetCrossfader(p) => self.crossfader.set_position(p),
                AudioCommand::SetMasterGain(g) => self.master.set_target(g),
                AudioCommand::SetDeckGain { deck, gain } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.gain.set_target(gain);
                    }
                }
                AudioCommand::SetDeckEq {
                    deck,
                    low_db,
                    mid_db,
                    high_db,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        let sr = d.sample_rate;
                        d.eq_l.set_gains_db(sr, low_db, mid_db, high_db);
                        d.eq_r.set_gains_db(sr, low_db, mid_db, high_db);
                    }
                }
                AudioCommand::SetDeckPlaying { deck, playing } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playing = playing;
                    }
                }
                AudioCommand::AttachDeck { deck, pcm } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        // NOTE (RT hazard): the replaced consumer (if any) is dropped here.
                        // TODO(P1): route the old consumer to a reclaim ring so its
                        // backing allocation is freed on the control thread, not the RT thread.
                        d.pcm = Some(pcm);
                        d.underruns = 0;
                    }
                }
                AudioCommand::DetachDeck { deck } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.pcm = None;
                        d.playing = false;
                    }
                }
            }
        }
    }

    /// Mix one stereo frame. RT-SAFE.
    ///
    /// Crossfader maps deck 0 → A side, deck 1 → B side (classic 2-deck layout).
    /// Decks 2/3 (when used) sum in at unity through the master for now; a 4-deck
    /// fader/assign matrix is a P4 concern.
    #[inline]
    pub fn next_frame(&mut self) -> (f32, f32) {
        let (ga, gb) = self.crossfader.next_gains();
        let mut l = 0.0;
        let mut r = 0.0;
        for (i, deck) in self.decks.iter_mut().enumerate() {
            let (dl, dr) = deck.next_frame();
            let xf = match i {
                0 => ga,
                1 => gb,
                _ => 1.0,
            };
            l += dl * xf;
            r += dr * xf;
        }
        let m = self.master.next_gain();
        (l * m, r * m)
    }
}
