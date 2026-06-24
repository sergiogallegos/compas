//! `compas-audio` — the real-time audio engine.
//!
//! Threading model (see ARCHITECTURE.md §"Threading model" for the full picture):
//!
//! ```text
//!   control thread (Tauri commands)         audio callback thread (cpal, RT)
//!   ───────────────────────────────         ────────────────────────────────
//!   AudioEngine  ──cmd ring (rtrb)──▶  Mixer.drain_commands()  (lock-free)
//!   decoder thread ──Arc<DeckBuffer>──▶ DeckPlayer.pull()       (immutable)
//! ```
//!
//! The callback NEVER allocates, locks, or blocks. All parameter changes arrive as
//! [`AudioCommand`]s over a single-producer/single-consumer ring; local track audio is
//! installed as immutable [`DeckBuffer`]s and retired through the reclaim ring.

#![forbid(unsafe_code)]

mod cue;
mod engine;
mod input;
mod live;
mod mixer;
mod sampler;
mod waveform;

pub use cue::{
    open_booth_output, open_booth_output_with_latency, open_cue_output,
    open_cue_output_with_latency, output_device_names, CueOutput, MonitorLatency,
};
pub use engine::{AudioEngine, EngineConfig};
pub use input::{input_device_names, open_aux_input, AuxInput};
pub use live::{run_live_analysis, LiveBeatClock, LiveBeatSnapshot};
pub use mixer::{AudioCommand, DeckTelemetry, FilterMode, NUM_DECKS};
pub use sampler::NUM_PADS as NUM_SAMPLER_PADS;
pub use waveform::compute_peaks;

// Re-export so consumers building commands have the buffer type in one place.
pub use compas_core::DeckBuffer;
