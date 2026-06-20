//! `compas-core` — shared domain types with no I/O and no real-time-thread code.
//!
//! These types are used across the audio engine, the source layer, and the Tauri
//! command boundary, so they are deliberately small, `Clone`, and `serde`-friendly.

#![forbid(unsafe_code)]

mod buffer;
mod capabilities;
pub mod control;
mod error;
mod track;

pub use buffer::DeckBuffer;
pub use capabilities::{SourceCapabilities, SourceKind};
pub use control::{Behavior, ControlId, ControlSpec, Registry, Unit};
pub use error::{CompasError, Result};
pub use track::{MusicProvider, TrackMetadata};

/// Identifier for a deck. The MVP uses two decks; the engine is built for N.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeckId(pub u8);

impl DeckId {
    pub const A: DeckId = DeckId(0);
    pub const B: DeckId = DeckId(1);
}

impl std::fmt::Display for DeckId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "deck-{}", self.0)
    }
}
