//! `compas-dsp` — signal processing primitives.
//!
//! Two clearly separated halves:
//!
//! * [`rt`] — **real-time-safe** building blocks (biquads, EQ, crossfader, gain
//!   smoothing). Every `process*` method here is allocation-free, lock-free, and
//!   branch-bounded so it may be called from the audio callback. See the
//!   `RT-SAFE` doc-comment on each.
//! * [`analysis`] — **offline** analysis (BPM, key). These allocate and run on a
//!   worker thread, never on the audio thread.

#![forbid(unsafe_code)]

pub mod analysis;
pub mod fx;
pub mod live;
pub mod rt;

pub use fx::{Effect, FxChain, FxSlot};
pub use live::{LiveEstimate, LiveTracker};

pub use rt::{
    meta_map, Biquad, BiquadCoeffs, Bitcrusher, Crossfader, Delay, Flanger, GainSmoother, LinkType,
    Reverb, Synth, ThreeBandEq, TimeStretch, Waveform, XfaderMode,
};
