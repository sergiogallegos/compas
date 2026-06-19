//! The mixer and command protocol. Everything here runs on the audio thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use compas_core::DeckBuffer;
use compas_dsp::{
    Biquad, BiquadCoeffs, Crossfader, Delay, GainSmoother, Reverb, Synth, ThreeBandEq, TimeStretch,
    Waveform,
};
use rtrb::Producer;

/// Number of decks the engine mixes. MVP uses 2; the array is sized for 4 so the
/// extension to 4 decks needs no structural change.
pub const NUM_DECKS: usize = 4;

/// Max echo time the pre-allocated delay line can hold (2 s = 1 beat at 30 BPM / 2 beats
/// at 60 BPM — well past any musical echo).
const MAX_DELAY_SECS: f32 = 2.0;

/// Sync PLL: how hard the beat-phase error pulls the follower's read rate. The pull is
/// capped to ±8% (a musical pitch-bend range) so locking in is a smooth glide, not a click.
const SYNC_PHASE_GAIN: f64 = 1.0;
const SYNC_MAX_BEND: f64 = 0.08;

/// Beat phase in `[0, 1)`: fractional position of `playhead` within its current beat.
#[inline]
fn beat_phase(playhead: f64, offset: f64, interval: f64) -> f64 {
    if interval <= 0.0 {
        return 0.0;
    }
    let p = (playhead - offset) / interval;
    p - p.floor()
}

/// Per-deck DJ filter mode (the single HPF/LPF "filter knob").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    LowPass,
    HighPass,
}

/// Which side of the crossfader a deck is routed to (4-deck assign matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfaderAssign {
    A,
    Thru,
    B,
}

impl XfaderAssign {
    /// Map a small integer (from the UI/IPC) to an assignment.
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => XfaderAssign::A,
            2 => XfaderAssign::B,
            _ => XfaderAssign::Thru,
        }
    }
}

/// Commands sent from the control thread into the audio callback over an SPSC ring.
///
/// RT note: applying a command is O(1) and allocation-free. [`AudioCommand::LoadDeck`]
/// installs an `Arc<DeckBuffer>`; the Arc it replaces is pushed to the engine's reclaim
/// ring so it is dropped on the control thread, never freed on the RT path.
pub enum AudioCommand {
    SetCrossfader(f32),
    SetMasterGain(f32),
    SetDeckGain {
        deck: usize,
        gain: f32,
    },
    SetDeckEq {
        deck: usize,
        low_db: f32,
        mid_db: f32,
        high_db: f32,
    },
    SetDeckFilter {
        deck: usize,
        mode: FilterMode,
        cutoff_hz: f32,
        resonance: f32,
    },
    /// Configure the per-deck echo/delay insert. Engaging it (false→true) clears the
    /// delay line so audio from a previous on-period doesn't burst back.
    SetDeckEcho {
        deck: usize,
        active: bool,
        time_sec: f32,
        feedback: f32,
        mix: f32,
    },
    /// Configure the per-deck reverb insert. Engaging it (false→true) clears the tail.
    SetDeckReverb {
        deck: usize,
        active: bool,
        room_size: f32,
        mix: f32,
    },
    SetDeckPlaying {
        deck: usize,
        playing: bool,
    },
    /// Varispeed ratio: 1.0 = original tempo & pitch; 1.06 = +6% (faster, higher).
    SetDeckTempo {
        deck: usize,
        ratio: f64,
    },
    /// Toggle key-lock (master tempo): tempo changes keep the original pitch.
    SetDeckKeylock {
        deck: usize,
        active: bool,
    },
    /// Route a deck to a crossfader side (0 = A, 1 = thru, 2 = B) for 4-deck mixing.
    SetDeckXfaderAssign {
        deck: usize,
        assign: u8,
    },
    /// Pre-fader-listen (PFL): add/remove a deck from the headphone cue bus.
    SetDeckCue {
        deck: usize,
        active: bool,
    },
    /// Headphone cue/master blend: 0 = cue bus only, 1 = master only.
    SetCueMix(f32),
    /// Headphone output level (0..~1).
    SetCueVolume(f32),
    /// Begin pushing the headphone cue mix into `sink` (interleaved stereo f32 at device
    /// rate). The control thread owns the matching consumer + the 2nd output stream.
    StartCueOutput {
        sink: Producer<f32>,
    },
    /// Stop pushing the cue mix; dropping the producer lets the cue stream wind down.
    StopCueOutput,
    /// Seek to an absolute position in source frames.
    SeekDeck {
        deck: usize,
        frame: f64,
    },
    /// Set (and activate) a loop region in source frames.
    SetLoop {
        deck: usize,
        in_frame: f64,
        out_frame: f64,
        active: bool,
    },
    /// Toggle an existing loop on/off without changing its region.
    SetLoopActive {
        deck: usize,
        active: bool,
    },
    /// Drive the play-head from a jog-wheel/scratch gesture. While `active`, the deck
    /// reads at `speed` (1.0 = natural play rate; negative = reverse) regardless of the
    /// transport state, and the play-head clamps to the track instead of auto-stopping.
    SetScratch {
        deck: usize,
        active: bool,
        speed: f64,
    },
    /// Install a decoded track on a deck (does not auto-play; resets play-head to 0).
    /// `beat_offset`/`beat_interval` are the analyzed beatgrid in source frames (for sync).
    LoadDeck {
        deck: usize,
        buffer: Arc<DeckBuffer>,
        beat_offset: f64,
        beat_interval: f64,
    },
    UnloadDeck {
        deck: usize,
    },
    /// Update a deck's beatgrid (source frames) — e.g. after a manual grid-anchor nudge.
    SetBeatgrid {
        deck: usize,
        offset: f64,
        interval: f64,
    },
    /// Make `deck` a continuous sync follower of `master` (None = sync off).
    SetDeckSync {
        deck: usize,
        master: Option<usize>,
    },
    /// Begin tapping the master output into `sink` (interleaved stereo f32 at device rate).
    /// The control thread owns the matching consumer and writes it to a file.
    StartRecording {
        sink: Producer<f32>,
    },
    /// Stop tapping; dropping the producer signals the writer thread to finalize the file.
    StopRecording,
    /// Synth instrument note on (MIDI note 0..127, velocity 0..127; 0 velocity = note off).
    NoteOn {
        note: u8,
        velocity: u8,
    },
    NoteOff {
        note: u8,
    },
    AllNotesOff,
    SetSynthWaveform {
        index: u8,
    },
    SetSynthGain {
        gain: f32,
    },
}

/// Shared, lock-free telemetry the control thread samples to drive the UI (position,
/// playing state). Written once per audio block; read at UI rate.
pub struct DeckTelemetry {
    playhead_bits: [AtomicU64; NUM_DECKS],
    playing: [AtomicBool; NUM_DECKS],
    loaded: [AtomicBool; NUM_DECKS],
    /// Per-deck output peak (pre-crossfader), f32 bits in an AtomicU64-as-u32 slot.
    level_bits: [AtomicU64; NUM_DECKS],
    master_l_bits: AtomicU64,
    master_r_bits: AtomicU64,
    /// Audio-callback CPU load (processing time ÷ block duration), smoothed. ≥1.0 = overload.
    rt_load_bits: AtomicU64,
    /// Count of blocks that overran their real-time budget (potential underruns).
    xruns: AtomicU64,
}

impl DeckTelemetry {
    pub fn new() -> Self {
        DeckTelemetry {
            playhead_bits: std::array::from_fn(|_| AtomicU64::new(0)),
            playing: std::array::from_fn(|_| AtomicBool::new(false)),
            loaded: std::array::from_fn(|_| AtomicBool::new(false)),
            level_bits: std::array::from_fn(|_| AtomicU64::new(0)),
            master_l_bits: AtomicU64::new(0),
            master_r_bits: AtomicU64::new(0),
            rt_load_bits: AtomicU64::new(0),
            xruns: AtomicU64::new(0),
        }
    }

    /// Audio-thread CPU load, 0..~1 (≥1 means the callback is overrunning its budget).
    pub fn rt_load(&self) -> f32 {
        f64::from_bits(self.rt_load_bits.load(Ordering::Relaxed)) as f32
    }

    /// Number of blocks that overran their real-time budget since start.
    pub fn xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }

    /// Per-deck output peak in 0..~1 (linear). For VU meters.
    pub fn deck_level(&self, deck: usize) -> f32 {
        self.level_bits
            .get(deck)
            .map(|a| f64::from_bits(a.load(Ordering::Relaxed)) as f32)
            .unwrap_or(0.0)
    }

    /// Master output peak (left, right), linear 0..~1.
    pub fn master_level(&self) -> (f32, f32) {
        (
            f64::from_bits(self.master_l_bits.load(Ordering::Relaxed)) as f32,
            f64::from_bits(self.master_r_bits.load(Ordering::Relaxed)) as f32,
        )
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
    /// Jog-wheel scratch: when active, `scratch_speed` drives the play-head instead of
    /// `tempo`, and audio plays regardless of the transport state.
    scratching: bool,
    scratch_speed: f64,
    /// Key-lock (master tempo): when on, the play-head is read through `stretch` so tempo
    /// changes without pitch. `stretch_engaged` tracks the previous frame's read mode so a
    /// jump into stretched reading (toggle/seek/scratch-release) re-primes the stretcher.
    keylock: bool,
    stretch: TimeStretch,
    stretch_engaged: bool,
    gain: GainSmoother,
    eq_l: ThreeBandEq,
    eq_r: ThreeBandEq,
    filter_l: Biquad,
    filter_r: Biquad,
    filter_active: bool,
    /// Crossfader routing (4-deck assign matrix): A side, B side, or straight through.
    xfader_assign: XfaderAssign,
    /// Pre-fader-listen: when true, this deck feeds the headphone cue bus regardless of the
    /// crossfader/master. Independent of `xfader_assign`.
    cue: bool,
    /// Echo/delay insert (post-EQ). The delay line is pre-allocated; toggling only flips
    /// `echo_active` and clears the line on engage.
    echo: Delay,
    echo_active: bool,
    /// Reverb insert (post-echo). Buffers pre-allocated; toggling flips `reverb_active`.
    reverb: Reverb,
    reverb_active: bool,
    loop_active: bool,
    loop_in: f64,
    loop_out: f64,
    device_rate: f32,
    /// Beatgrid in source frames: phase of the first beat, and frames per beat. Used by the
    /// continuous sync PLL; 0 interval means "no grid" (sync disabled for this deck).
    beat_offset: f64,
    beat_interval: f64,
    /// When this deck is a sync follower, the index of the deck it tracks.
    sync_master: Option<usize>,
    /// Sync-controlled read rate (overrides `tempo` while engaged); set each block by the PLL.
    sync_tempo: Option<f64>,
}

impl DeckPlayer {
    fn new(device_rate: f32) -> Self {
        DeckPlayer {
            buffer: None,
            playhead: 0.0,
            base_ratio: 1.0,
            tempo: 1.0,
            playing: false,
            scratching: false,
            scratch_speed: 0.0,
            keylock: false,
            stretch: TimeStretch::new(),
            stretch_engaged: false,
            gain: GainSmoother::new(1.0, device_rate, 8.0),
            eq_l: ThreeBandEq::new(device_rate),
            eq_r: ThreeBandEq::new(device_rate),
            filter_l: Biquad::new(BiquadCoeffs::IDENTITY),
            filter_r: Biquad::new(BiquadCoeffs::IDENTITY),
            filter_active: false,
            xfader_assign: XfaderAssign::Thru,
            cue: false,
            echo: Delay::new(device_rate, MAX_DELAY_SECS),
            echo_active: false,
            reverb: Reverb::new(device_rate),
            reverb_active: false,
            loop_active: false,
            loop_in: 0.0,
            loop_out: 0.0,
            device_rate,
            beat_offset: 0.0,
            beat_interval: 0.0,
            sync_master: None,
            sync_tempo: None,
        }
    }

    /// Pull and process one stereo frame. RT-SAFE.
    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        let g = self.gain.next_gain(); // advance smoother even when paused (click-free unpause)

        let Some(buf) = self.buffer.as_ref() else {
            return (0.0, 0.0);
        };
        let frames = buf.frames();
        if frames == 0 {
            return (0.0, 0.0);
        }

        // Scratching overrides the transport: a jog gesture drives the play-head and
        // produces audio whether or not the deck is "playing".
        if !self.playing && !self.scratching {
            return (0.0, 0.0);
        }
        // End-of-track auto-stops normal playback, but never scratching (so you can
        // scrub back from the end).
        if !self.scratching && self.playhead >= frames as f64 {
            self.playing = false;
            return (0.0, 0.0);
        }

        let max = frames as f64 - 1.0;

        // Read mode: with key-lock on, stream through the WSOLA stretcher (tempo without
        // pitch); otherwise read directly. Scratching always uses the direct (varispeed)
        // path. Re-prime the stretcher whenever we (re)enter stretched reading, since the
        // play-head may have jumped (toggle / seek / scratch release).
        let engaged = self.keylock && !self.scratching;
        if engaged && !self.stretch_engaged {
            self.stretch.reset();
        }
        self.stretch_engaged = engaged;

        let (mut l, mut r) = if engaged {
            self.stretch
                .next_frame(&buf.samples, frames, self.base_ratio, self.playhead)
        } else {
            interp_stereo(&buf.samples, frames, self.playhead.clamp(0.0, max))
        };

        if self.scratching {
            // Hand-driven read rate (can be negative); clamp to the track bounds.
            self.playhead = (self.playhead + self.base_ratio * self.scratch_speed).clamp(0.0, max);
        } else {
            // Sync (when engaged) overrides the user tempo with the PLL's read rate.
            let rate = self.sync_tempo.unwrap_or(self.tempo);
            self.playhead += self.base_ratio * rate;
            // Beat loop: wrap the play-head back to loop-in once it passes loop-out.
            if self.loop_active && self.loop_out > self.loop_in {
                let len = self.loop_out - self.loop_in;
                while self.playhead >= self.loop_out {
                    self.playhead -= len;
                }
            }
        }

        if self.filter_active {
            l = self.filter_l.process(l);
            r = self.filter_r.process(r);
        }
        l = self.eq_l.process(l);
        r = self.eq_r.process(r);
        if self.echo_active {
            let (el, er) = self.echo.process(l, r);
            l = el;
            r = er;
        }
        if self.reverb_active {
            let (rl, rr) = self.reverb.process(l, r);
            l = rl;
            r = rr;
        }
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
    // Per-block peak accumulators (reset on publish).
    peak_deck: [f32; NUM_DECKS],
    peak_l: f32,
    peak_r: f32,
    /// When recording, the master output is pushed here for the writer thread to drain.
    record_sink: Option<Producer<f32>>,
    /// Headphone cue bus: blend (0 = cued decks only, 1 = master only) and output level.
    cue_mix: f32,
    cue_vol: GainSmoother,
    /// When cue monitoring is on, the headphone mix is pushed here for the 2nd output stream.
    cue_sink: Option<Producer<f32>>,
    /// Smoothed audio-callback load, and the running overrun count.
    rt_load: f32,
    xrun_count: u64,
    /// Polyphonic synth instrument, summed into the master (post-deck, pre-master-gain).
    synth: Synth,
}

impl Mixer {
    pub fn new(
        device_rate: f32,
        commands: rtrb::Consumer<AudioCommand>,
        reclaim: Producer<Arc<DeckBuffer>>,
        telemetry: Arc<DeckTelemetry>,
    ) -> Self {
        Mixer {
            decks: std::array::from_fn(|i| {
                let mut d = DeckPlayer::new(device_rate);
                // Default routing: deck 0 → A side, deck 1 → B side, decks 2/3 → through.
                d.xfader_assign = match i {
                    0 => XfaderAssign::A,
                    1 => XfaderAssign::B,
                    _ => XfaderAssign::Thru,
                };
                d
            }),
            crossfader: Crossfader::new(device_rate),
            master: GainSmoother::new(0.85, device_rate, 10.0),
            commands,
            reclaim,
            telemetry,
            device_rate,
            peak_deck: [0.0; NUM_DECKS],
            peak_l: 0.0,
            peak_r: 0.0,
            record_sink: None,
            // Default cue blend at the "cue" end so PFL'd decks are audible immediately.
            cue_mix: 0.0,
            cue_vol: GainSmoother::new(0.8, device_rate, 10.0),
            cue_sink: None,
            rt_load: 0.0,
            xrun_count: 0,
            synth: Synth::new(device_rate),
        }
    }

    /// Publish the audio-callback load (processing time ÷ block budget). Smoothed for a
    /// stable UI readout; each overrun (load ≥ 1.0) bumps the xrun counter. RT-SAFE.
    #[inline]
    pub fn publish_rt_load(&mut self, load: f32) {
        // Fast attack, slow release so brief spikes are visible but the readout is steady.
        self.rt_load = if load > self.rt_load {
            load
        } else {
            self.rt_load * 0.9 + load * 0.1
        };
        if load >= 1.0 {
            self.xrun_count += 1;
        }
        self.telemetry
            .rt_load_bits
            .store((self.rt_load as f64).to_bits(), Ordering::Relaxed);
        self.telemetry
            .xruns
            .store(self.xrun_count, Ordering::Relaxed);
    }

    /// Retire a deck's old buffer to the control thread for dropping (RT-safe: if the
    /// reclaim ring is unexpectedly full we drop here, which is rare and bounded).
    #[inline]
    fn retire(&mut self, buffer: Option<Arc<DeckBuffer>>) {
        if let Some(b) = buffer {
            let _ = self.reclaim.push(b);
        }
    }

    /// Continuous beat-sync PLL. For each follower deck, rate-match its beat rate to its
    /// master's and nudge its read rate (±[`SYNC_MAX_BEND`]) to null the beat-phase error.
    /// Sets `sync_tempo` per follower; clears it when sync is off or unusable. RT-SAFE.
    #[inline]
    fn update_sync(&mut self) {
        // Snapshot what the PLL needs (avoids aliasing master/follower borrows).
        let mut snap = [(0.0f64, 0.0f64, 0.0f64, 0.0f64, false); NUM_DECKS];
        for (i, d) in self.decks.iter().enumerate() {
            // A master plays at its own user rate.
            let adv = d.base_ratio * d.tempo;
            snap[i] = (d.playhead, d.beat_offset, d.beat_interval, adv, d.playing);
        }
        for (i, d) in self.decks.iter_mut().enumerate() {
            let Some(m) = d.sync_master else {
                d.sync_tempo = None;
                continue;
            };
            let (m_ph, m_off, m_int, m_adv, m_playing) = snap.get(m).copied().unwrap_or_default();
            if m == i || m_int <= 0.0 || d.beat_interval <= 0.0 || !m_playing || d.base_ratio <= 0.0 {
                d.sync_tempo = None;
                continue;
            }
            // Rate-match: follower advances so its beats tick at the master's beat rate.
            let master_beat_rate = m_adv / m_int; // beats per output sample
            let base_adv = master_beat_rate * d.beat_interval; // follower frames per sample
            // Phase error (master − follower), wrapped to the nearest beat [−0.5, 0.5].
            let mut err = beat_phase(m_ph, m_off, m_int) - beat_phase(d.playhead, d.beat_offset, d.beat_interval);
            err -= err.round();
            let bend = (SYNC_PHASE_GAIN * err).clamp(-SYNC_MAX_BEND, SYNC_MAX_BEND);
            d.sync_tempo = Some((base_adv * (1.0 + bend)) / d.base_ratio);
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
                AudioCommand::SetDeckEcho {
                    deck,
                    active,
                    time_sec,
                    feedback,
                    mix,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.echo.set_time_sec(time_sec);
                        d.echo.set_feedback(feedback);
                        d.echo.set_mix(mix);
                        if active && !d.echo_active {
                            d.echo.clear(); // fresh line on engage
                        }
                        d.echo_active = active;
                    }
                }
                AudioCommand::SetDeckReverb {
                    deck,
                    active,
                    room_size,
                    mix,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.reverb.set_room_size(room_size);
                        d.reverb.set_mix(mix);
                        if active && !d.reverb_active {
                            d.reverb.clear(); // fresh tail on engage
                        }
                        d.reverb_active = active;
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
                AudioCommand::SetDeckKeylock { deck, active } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.keylock = active;
                        // Force a re-prime on the next stretched frame.
                        d.stretch_engaged = false;
                    }
                }
                AudioCommand::SetDeckXfaderAssign { deck, assign } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.xfader_assign = XfaderAssign::from_index(assign);
                    }
                }
                AudioCommand::SetDeckCue { deck, active } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.cue = active;
                    }
                }
                AudioCommand::SetCueMix(m) => self.cue_mix = m.clamp(0.0, 1.0),
                AudioCommand::SetCueVolume(v) => self.cue_vol.set_target(v.max(0.0)),
                AudioCommand::StartCueOutput { sink } => self.cue_sink = Some(sink),
                AudioCommand::StopCueOutput => self.cue_sink = None,
                AudioCommand::SeekDeck { deck, frame } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playhead = frame.max(0.0);
                        // The play-head jumped — re-prime the stretcher on the next frame.
                        d.stretch_engaged = false;
                    }
                }
                AudioCommand::LoadDeck {
                    deck,
                    buffer,
                    beat_offset,
                    beat_interval,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.base_ratio = buffer.source_rate as f64 / self.device_rate as f64;
                        d.playhead = 0.0;
                        d.tempo = 1.0;
                        d.playing = false;
                        d.scratching = false;
                        d.scratch_speed = 0.0;
                        d.keylock = false;
                        d.stretch_engaged = false;
                        d.echo_active = false;
                        d.echo.clear();
                        d.reverb_active = false;
                        d.reverb.clear();
                        d.loop_active = false;
                        d.beat_offset = beat_offset;
                        d.beat_interval = beat_interval;
                        d.sync_master = None;
                        d.sync_tempo = None;
                        let old = d.buffer.replace(buffer);
                        self.retire(old);
                    }
                }
                AudioCommand::SetBeatgrid {
                    deck,
                    offset,
                    interval,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.beat_offset = offset;
                        d.beat_interval = interval;
                    }
                }
                AudioCommand::SetDeckSync { deck, master } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.sync_master = master;
                        d.sync_tempo = None;
                    }
                    // Break any A↔B cycle: if the new master was following this deck, stop it.
                    if let Some(m) = master {
                        if m != deck {
                            if let Some(md) = self.decks.get_mut(m) {
                                if md.sync_master == Some(deck) {
                                    md.sync_master = None;
                                    md.sync_tempo = None;
                                }
                            }
                        }
                    }
                }
                AudioCommand::SetLoop {
                    deck,
                    in_frame,
                    out_frame,
                    active,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.loop_in = in_frame.max(0.0);
                        d.loop_out = out_frame.max(0.0);
                        d.loop_active = active && out_frame > in_frame;
                    }
                }
                AudioCommand::SetLoopActive { deck, active } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.loop_active = active && d.loop_out > d.loop_in;
                    }
                }
                AudioCommand::SetScratch {
                    deck,
                    active,
                    speed,
                } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.scratching = active;
                        d.scratch_speed = speed.clamp(-16.0, 16.0);
                    }
                }
                AudioCommand::StartRecording { sink } => {
                    self.record_sink = Some(sink);
                }
                AudioCommand::StopRecording => {
                    // Dropping the producer makes the writer thread see `is_abandoned`
                    // and finalize the WAV. Drop happens here on the audio thread, but the
                    // consumer is still alive so it only decrements a refcount (RT-safe).
                    self.record_sink = None;
                }
                AudioCommand::NoteOn { note, velocity } => self.synth.note_on(note, velocity),
                AudioCommand::NoteOff { note } => self.synth.note_off(note),
                AudioCommand::AllNotesOff => self.synth.all_notes_off(),
                AudioCommand::SetSynthWaveform { index } => {
                    self.synth.set_waveform(Waveform::from_index(index))
                }
                AudioCommand::SetSynthGain { gain } => self.synth.set_gain(gain),
                AudioCommand::UnloadDeck { deck } => {
                    if let Some(d) = self.decks.get_mut(deck) {
                        d.playing = false;
                        d.scratching = false;
                        d.scratch_speed = 0.0;
                        d.keylock = false;
                        d.stretch_engaged = false;
                        d.echo_active = false;
                        d.echo.clear();
                        d.reverb_active = false;
                        d.reverb.clear();
                        d.playhead = 0.0;
                        d.loop_active = false;
                        d.sync_master = None;
                        d.sync_tempo = None;
                        d.beat_interval = 0.0;
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
        self.update_sync();
        let (ga, gb) = self.crossfader.next_gains();
        let mut l = 0.0;
        let mut r = 0.0;
        // Headphone cue (PFL) bus: cued decks summed pre-crossfader, pre-master.
        let mut cl = 0.0;
        let mut cr = 0.0;
        for (i, deck) in self.decks.iter_mut().enumerate() {
            let (dl, dr) = deck.next_frame();
            let deck_peak = dl.abs().max(dr.abs());
            if deck_peak > self.peak_deck[i] {
                self.peak_deck[i] = deck_peak;
            }
            if deck.cue {
                cl += dl;
                cr += dr;
            }
            let xf = match deck.xfader_assign {
                XfaderAssign::A => ga,
                XfaderAssign::B => gb,
                XfaderAssign::Thru => 1.0,
            };
            l += dl * xf;
            r += dr * xf;
        }
        // Synth instrument sits on the master bus (centered), so it's always audible and
        // captured by the recorder, independent of the crossfader.
        let sy = self.synth.process();
        l += sy;
        r += sy;
        let m = self.master.next_gain();
        let (ol, or) = (l * m, r * m);
        self.peak_l = self.peak_l.max(ol.abs());
        self.peak_r = self.peak_r.max(or.abs());
        // Recording tap: push the master frame for the writer thread. Push both samples
        // together (or neither) so L/R never split across a full-ring drop. RT-safe.
        if let Some(sink) = self.record_sink.as_mut() {
            if sink.slots() >= 2 {
                let _ = sink.push(ol);
                let _ = sink.push(or);
            }
        }
        // Headphone cue tap: blend the cue bus with the master (cue_mix) at the headphone
        // level, and push for the 2nd output stream. Advance the smoother every frame
        // (click-free) whether or not the sink is connected. RT-safe.
        let cvol = self.cue_vol.next_gain();
        if let Some(sink) = self.cue_sink.as_mut() {
            let mix = self.cue_mix;
            let hl = (cl * (1.0 - mix) + ol * mix) * cvol;
            let hr = (cr * (1.0 - mix) + or * mix) * cvol;
            if sink.slots() >= 2 {
                let _ = sink.push(hl);
                let _ = sink.push(hr);
            }
        }
        (ol, or)
    }

    /// Publish per-deck position/state to shared telemetry. Call once per audio block.
    /// RT-SAFE (relaxed atomic stores only).
    #[inline]
    pub fn publish_telemetry(&mut self) {
        for (i, d) in self.decks.iter().enumerate() {
            self.telemetry.playhead_bits[i].store(d.playhead.to_bits(), Ordering::Relaxed);
            self.telemetry.playing[i].store(d.playing, Ordering::Relaxed);
            self.telemetry.loaded[i].store(d.buffer.is_some(), Ordering::Relaxed);
            self.telemetry.level_bits[i]
                .store((self.peak_deck[i] as f64).to_bits(), Ordering::Relaxed);
        }
        self.telemetry
            .master_l_bits
            .store((self.peak_l as f64).to_bits(), Ordering::Relaxed);
        self.telemetry
            .master_r_bits
            .store((self.peak_r as f64).to_bits(), Ordering::Relaxed);
        // Reset accumulators for the next block.
        self.peak_deck = [0.0; NUM_DECKS];
        self.peak_l = 0.0;
        self.peak_r = 0.0;
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

    fn ramp_deck() -> DeckPlayer {
        // A 100-frame ramp so the play-head's audio reflects its position.
        let mut samples = Vec::with_capacity(200);
        for i in 0..100 {
            samples.push(i as f32);
            samples.push(i as f32);
        }
        let mut d = DeckPlayer::new(48_000.0);
        d.buffer = Some(Arc::new(DeckBuffer::new(samples, 48_000)));
        d.base_ratio = 1.0;
        d
    }

    #[test]
    fn scratch_plays_while_paused_and_moves_playhead() {
        let mut d = ramp_deck();
        d.playhead = 40.0;
        d.playing = false; // paused — but a scratch still sounds
        d.scratching = true;
        d.scratch_speed = 1.0;
        let (l, _r) = d.next_frame();
        assert!(l != 0.0, "scratch should produce audio while paused");
        assert!(d.playhead > 40.0, "forward scratch should advance the play-head");
    }

    #[test]
    fn reverse_scratch_runs_backward_and_clamps_at_zero() {
        let mut d = ramp_deck();
        d.playhead = 1.0;
        d.scratching = true;
        d.scratch_speed = -4.0;
        for _ in 0..10 {
            d.next_frame();
        }
        assert!(d.playhead >= 0.0, "reverse scratch must not run past frame 0");
        assert!(d.playhead < 1.0, "reverse scratch should have moved backward");
    }

    #[test]
    fn sync_locks_follower_phase_to_master() {
        let (_ctx, crx) = rtrb::RingBuffer::<AudioCommand>::new(8);
        let (rtx, _rrx) = rtrb::RingBuffer::<Arc<DeckBuffer>>::new(8);
        let mut mixer = Mixer::new(48_000.0, crx, rtx, Arc::new(DeckTelemetry::new()));
        let buf = Arc::new(DeckBuffer::new(vec![0.0; 2 * 480_000], 48_000));
        let interval = 24_000.0; // 0.5 s/beat = 120 BPM @ 48 kHz
        for d in mixer.decks.iter_mut() {
            d.buffer = Some(buf.clone());
            d.base_ratio = 1.0;
            d.playing = true;
            d.beat_offset = 0.0;
            d.beat_interval = interval;
        }
        // Follower (deck 1) starts a quarter-beat out of phase and tracks the master (deck 0).
        mixer.decks[1].playhead = 6_000.0;
        mixer.decks[1].sync_master = Some(0);
        for _ in 0..192_000 {
            mixer.next_frame(); // ~4 s
        }
        let mp = beat_phase(mixer.decks[0].playhead, 0.0, interval);
        let fp = beat_phase(mixer.decks[1].playhead, 0.0, interval);
        let mut err = mp - fp;
        err -= err.round();
        assert!(err.abs() < 0.02, "follower not phase-locked: err={err}");
    }

    #[test]
    fn cue_bus_sums_only_cued_decks_into_the_sink() {
        let (_ctx, crx) = rtrb::RingBuffer::<AudioCommand>::new(8);
        let (rtx, _rrx) = rtrb::RingBuffer::<Arc<DeckBuffer>>::new(8);
        let mut mixer = Mixer::new(48_000.0, crx, rtx, Arc::new(DeckTelemetry::new()));
        // Two decks playing a DC ramp so each yields nonzero audio.
        let buf = Arc::new(DeckBuffer::new(vec![0.5; 2 * 1000], 48_000));
        for d in mixer.decks.iter_mut().take(2) {
            d.buffer = Some(buf.clone());
            d.base_ratio = 1.0;
            d.playing = true;
            d.xfader_assign = XfaderAssign::Thru;
        }
        // Cue only deck 1; full cue (no master bleed), unity headphone level.
        mixer.decks[1].cue = true;
        mixer.cue_mix = 0.0;
        mixer.cue_vol = GainSmoother::new(1.0, 48_000.0, 10.0);
        let (cue_tx, mut cue_rx) = rtrb::RingBuffer::<f32>::new(64);
        mixer.cue_sink = Some(cue_tx);

        let (ml, _mr) = mixer.next_frame();
        let hl = cue_rx.pop().expect("cue L pushed");
        let _hr = cue_rx.pop().expect("cue R pushed");
        // Master carries both decks; the cue bus carries only deck 1 → strictly smaller.
        assert!(hl.abs() > 0.0, "cued deck should reach the headphones");
        assert!(hl.abs() < ml.abs(), "cue bus (1 deck) < master (2 decks)");
    }

    #[test]
    fn no_cue_sink_means_no_push_but_still_advances() {
        let (_ctx, crx) = rtrb::RingBuffer::<AudioCommand>::new(8);
        let (rtx, _rrx) = rtrb::RingBuffer::<Arc<DeckBuffer>>::new(8);
        let mut mixer = Mixer::new(48_000.0, crx, rtx, Arc::new(DeckTelemetry::new()));
        // With no cue sink installed, next_frame must not panic and the master still mixes.
        let buf = Arc::new(DeckBuffer::new(vec![0.3; 2 * 100], 48_000));
        mixer.decks[0].buffer = Some(buf);
        mixer.decks[0].playing = true;
        mixer.decks[0].cue = true; // cued, but nowhere to send
        let _ = mixer.next_frame();
    }

    #[test]
    fn scratch_does_not_auto_stop_at_end() {
        let mut d = ramp_deck();
        d.playhead = 99.0;
        d.playing = true;
        d.scratching = true;
        d.scratch_speed = 4.0;
        for _ in 0..5 {
            d.next_frame();
        }
        assert!(d.scratching, "scratch stays engaged at the end of the track");
        assert!(d.playhead <= 99.0, "play-head clamps to the last frame");
    }
}
