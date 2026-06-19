//! `compas-audio` — the real-time audio engine.
//!
//! Threading model (see ARCHITECTURE.md §"Threading model" for the full picture):
//!
//! ```text
//!   control thread (Tauri commands)         audio callback thread (cpal, RT)
//!   ───────────────────────────────         ────────────────────────────────
//!   AudioEngine  ──cmd ring (rtrb)──▶  Mixer.drain_commands()  (lock-free)
//!   decoder thread ──PCM ring (rtrb)──▶  DeckAudio.pull()       (lock-free)
//! ```
//!
//! The callback NEVER allocates, locks, or blocks. All parameter changes arrive as
//! [`AudioCommand`]s over a single-producer/single-consumer ring; all audio arrives
//! over per-deck SPSC rings filled by decoder threads.

#![forbid(unsafe_code)]

mod cue;
mod engine;
mod mixer;
mod sampler;
mod waveform;

pub use cue::{open_cue_output, output_device_names, CueOutput};
pub use engine::{AudioEngine, EngineConfig};
pub use mixer::{AudioCommand, DeckTelemetry, FilterMode, NUM_DECKS};
pub use sampler::NUM_PADS as NUM_SAMPLER_PADS;
pub use waveform::compute_peaks;

// Re-export so consumers building commands have the buffer type in one place.
pub use compas_core::DeckBuffer;
