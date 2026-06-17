//! The mixer and command protocol. Everything here runs on the audio thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use compas_core::DeckBuffer;
use compas_dsp::{Biquad, BiquadCoeffs, Crossfader, GainSmoother, ThreeBandEq};
use rtrb::Producer;

/// Number of decks the engine mixes. MVP uses 2; the array is sized for 4 so the
/// extension to 4 decks needs no structural change.
pub const NUM_DECKS: usize = 4;

/// Per-deck DJ filter mode (the single HPF/LPF "filter knob").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    LowPass,
    HighPass,
}

/// Commands sent from the control thread into the audio callback over an SPSC ring.
///
/// RT note: applying a command is O(1) and allocation-free. [`AudioCommand::LoadDeck`]
/// installs an `Arc<DeckBuffer>`; the Arc it replaces is pushed to the engine's reclaim
/// ring so it is dropped on the control thread, never freed on the RT path.
pub enum AudioCommand {
    SetCrossfader(f32),
    SetMasterGain(f32),
    SetDeckGain { deck: usize, gain: f32 },
    SetDeckEq { deck: usize, low_db: f32, mid_db: f32, high_db: f32 },
    SetDeckFilter { deck: usize, mode: FilterMode, cutoff_hz: f32, resonance: f32 },
    SetDeckPlaying { deck: usize, playing: bool },
    /// Varispeed ratio: 1.0 = original tempo & pitch; 1.06 = +6% (faster, higher).
    SetDeckTempo { deck: usize, ratio: f64 },
    /// Seek to an absolute position in source frames.
    SeekDeck { deck: usize, frame: f64 },
    /// Install a decoded track on a deck (does not auto-play; resets play-head to 0).
    LoadDeck { deck: usize, buffer: Arc<DeckBuffer> },
    UnloadDeck { deck: usize },
}

/// Shared, lock-free telemetry the control thread samples to drive the UI (position,
/// playing state). Written once per audio block; read at UI rate.
pub struct DeckTelemetry {
    playhead_bits: [AtomicU64; NUM_DECKS],
    playing: [AtomicBool; NUM_DECKS],
    loaded: [AtomicBool; NUM_DECKS],
}

impl DeckTelemetry {
    pub fn new() -> Self {
        DeckTelemetry {
            playhead_bits: std::array::from_fn(|_| AtomicU64::new(0)),
            playing: std::array::from_fn(|_| AtomicBool::new(false)),
            loaded: std::array::from_fn(|_| AtomicBool::new(false)),
        }
    }

    /// Current play-head for `deck`, in source frames.
    pub fn playhead_frames(&self, deck: usize) -> f64 {
        self.playhead_bits
            .get(deck)
            .map(|a| f64::from_bits(a.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    pub fn is_playing(&self, deck: usize) -> bool {
        self.playing
            .get(deck)
            .map(|a| a.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    pub fn is_loaded(&self, deck: usize) -> bool {
        self.loaded
            .get(deck)
            .map(|a| a.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

impl Default for DeckTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-deck audio state living on the audio thread.
struct DeckPlayer {
    buffer: Option<Arc<DeckBuffer>>,
    /// Play-head in source frames (fractional → enables varispeed/scratch).
    playhead: f64,
    /// source_rate / device_rate — keeps pitch correct across rate mismatch.
    base_ratio: f64,
    /// Varispeed ratio (1.0 = original). Combined advance = base_ratio * tempo.
    tempo: f64,
    playing: bool,
    gain: GainSmoother,
    eq_l: ThreeBandEq,
    eq_r: ThreeBandEq,
    filter_l: Biquad,
    filter_r: Biquad,
    filter_active: bool,
    device_rate: f32,
}

impl DeckPlayer {
    fn new(device_rate: f32) -> Self {
        DeckPlayer {
            buffer: None,
            playhead: 0.0,
            base_ratio: 1.0,
            tempo: 1.0,
            playing: false,
            gain: GainSmoother::new(1.0, device_rate, 8.0),
            eq_l: ThreeBandEq::new(device_rate),
            eq_r: ThreeBandEq::new(device_rate),
            filter_l: Biquad::new(BiquadCoeffs::IDENTITY),
            filter_r: Biquad::new(BiquadCoeffs::IDENTITY),
            filter_active: false,
            device_rate,
        }
    }

    /// Pull and process one stereo frame. RT-SAFE.
    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        let g = self.gain.next_gain(); // advance smoother even when paused (click-free unpause)

        if !self.playing {
            return (0.0, 0.0);
        }
        let Some(buf) = self.buffer.as_ref() else {
            return (0.0, 0.0);
        };
        let frames = buf.frames();
        if frames == 0 || self.playhead >= frames as f64 {
            self.playing = false;
            return (0.0, 0.0);
        }

        let (mut l, mut r) = interp_stereo(&buf.samples, frames, self.playhead);
        self.playhead += self.base_ratio * self.tempo;

        if self.filter_active {
            l = self.filter_l.process(l);
            r = self.filter_r.process(r);
        }
        l = self.eq_l.process(l);
        r = self.eq_r.process(r);
        (l * g, r * g)
    }

    fn set_filter(&mut self, mode: FilterMode, cutoff_hz: f32, resonance: f32) {
        let q = resonance.max(0.1);
        match mode {
            FilterMode::Off => {
                self.filter_active = false;
                self.filter_l.set_coeffs(BiquadCoeffs::IDENTITY);
                self.filter_r.set_coeffs(BiquadCoeffs::IDENTITY);
            }
            FilterMode::LowPass => {
                let c = BiquadCoeffs::low_pass(cutoff_hz, self.device_rate, q);
                self.filter_l.set_coeffs(c);
                self.filter_r.set_coeffs(c);
                self.filter_active = true;
            }
            FilterMode::HighPass => {
                let c = BiquadCoeffs::high_pass(cutoff_hz, self.device_rate, q);
                self.filter_l.set_coeffs(c);
                self.filter_r.set_coeffs(c);
                self.filter_active = true;
            }
        }
    }
}

/// 4-point cubic Hermite (Catmull-Rom) interpolation at a fractional frame position.
/// Much cleaner than linear for varispeed/scratch; cheap enough for the RT path.
#[inline]
fn interp_stereo(samples: &[f32], frames: usize, pos: f64) -> (f32, f32) {
    let i = pos.floor() as isize;
    let t = (pos - i as f64) as f32;
    let at = |frame: isize, ch: usize| -> f32 {
        let f = frame.clamp(0, frames as isize - 1) as usize;
        samples[f * 2 + ch]
    };
    let l = hermite(at(i - 1, 0), at(i, 0), at(i + 1, 0), at(i + 2, 0), t);
    let r = hermite(at(i - 1, 1), at(i, 1), at(i + 1, 1), at(i + 2, 1), t);
    (l, r)
}

#[inline]
fn hermite(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let c0 = p1;
    let c1 = 0.5 * (p2 - p0);
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    ((c3 * t + c2) * t + c1) * t + c0
}

/// The audio-thread mixer: N decks → crossfader → master gain → output.
pub struct Mixer {
    decks: [DeckPlayer; NUM_DECKS],
    crossfader: Crossfader,
    master: GainSmoother,
    commands: rtrb::Consumer<AudioCommand>,
    reclaim: Producer<Arc<DeckBuffer>>,
    telemetry: Arc<DeckTelemetry>,
    device_rate: f32,
}

impl Mixer {
    pub fn new(
        device_rate: f32,
        commands: rtrb::Consumer<AudioCommand>,
        reclaim: Producer<Arc<DeckBuffer>>,
        telemetry: Arc<DeckTelemetry>,
    ) -> Self {
        Mixer {
            decks: std::array::from_fn(|_| DeckPlayer::new(device_rate)),
            crossfader: Crossfader::new(device_rate),
            master: GainSmoother::new(0.85, device_rate, 10.0),
            commands,
            reclaim,
            telemetry,
            device_rate,
        }
    }

    /// Retire a deck's old buffer to the control thread for dropping (RT-safe: if the
    /// reclaim ring is unexpectedly full we drop here, which is rare and bounded).
    #[inline]
    fn retire(&mut self, buffer: Option<Arc<DeckBuffer>>) {
        if let Some(b) = buffer {
            let _ = self.reclaim.push(b);
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
                        let sr = d.device_rate;
                        d.eq_l.set_gains_db(sr, low_db, mid_db, high_db);
                        d.eq_r.set_gains_db(sr, low_db, mid_db, high_db);
                    }
                }
                AudioCommand::SetDeckFilter {
                    deck,
                    mode,
                    cutoff_hz,
                    resonance,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.set_filter(mode, cutoff_hz, resonance);
                    }
                }
                AudioCommand::SetDeckPlaying { deck, playing } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playing = playing;
                    }
                }
                AudioCommand::SetDeckTempo { deck, ratio } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.tempo = ratio.clamp(0.05, 4.0);
                    }
                }
                AudioCommand::SeekDeck { deck, frame } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playhead = frame.max(0.0);
                    }
                }
                AudioCommand::LoadDeck { deck, buffer } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.base_ratio = buffer.source_rate as f64 / self.device_rate as f64;
                        d.playhead = 0.0;
                        d.tempo = 1.0;
                        d.playing = false;
                        let old = d.buffer.replace(buffer);
                        self.retire(old);
                    }
                }
                AudioCommand::UnloadDeck { deck } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playing = false;
                        d.playhead = 0.0;
                        let old = d.buffer.take();
                        self.retire(old);
                    }
                }
            }
        }
    }

    /// Mix one stereo frame. RT-SAFE. Deck 0 → crossfader A, deck 1 → B; decks 2/3
    /// sum through at unity (4-deck fader matrix is a P4 concern).
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

    /// Publish per-deck position/state to shared telemetry. Call once per audio block.
    /// RT-SAFE (relaxed atomic stores only).
    #[inline]
    pub fn publish_telemetry(&self) {
        for (i, d) in self.decks.iter().enumerate() {
            self.telemetry.playhead_bits[i].store(d.playhead.to_bits(), Ordering::Relaxed);
            self.telemetry.playing[i].store(d.playing, Ordering::Relaxed);
            self.telemetry.loaded[i].store(d.buffer.is_some(), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermite_passes_through_control_points() {
        // At t=0 the result is p1; at t=1 it is p2.
        assert!((hermite(0.0, 1.0, 2.0, 3.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((hermite(0.0, 1.0, 2.0, 3.0, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn interp_reads_exact_frames_on_integers() {
        // interleaved stereo: frame0=(0,10) frame1=(1,11) frame2=(2,12)
        let s = [0.0, 10.0, 1.0, 11.0, 2.0, 12.0];
        let (l, r) = interp_stereo(&s, 3, 1.0);
        assert!((l - 1.0).abs() < 1e-5 && (r - 11.0).abs() < 1e-5);
    }

    #[test]
    fn interp_midpoint_is_between_neighbors() {
        let s = [0.0, 0.0, 2.0, 2.0, 4.0, 4.0, 6.0, 6.0];
        let (l, _r) = interp_stereo(&s, 4, 1.5);
        assert!(l > 2.0 && l < 4.0, "midpoint {l} not between 2 and 4");
    }
}
