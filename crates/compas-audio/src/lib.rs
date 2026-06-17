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

mod engine;
mod mixer;

pub use engine::{AudioEngine, DeckPcmProducer, EngineConfig};
pub use mixer::{AudioCommand, NUM_DECKS};
