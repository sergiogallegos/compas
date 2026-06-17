//! `compas-sources` — the [`AudioSource`] abstraction and its implementations.
//!
//! This is the architectural keystone (see ARCHITECTURE.md §"Source abstraction").
//! A source is one of two fundamentally different things:
//!
//! * [`LocalFileSource`] — decodes a DRM-free file to PCM. Provides sample buffers,
//!   so the full DSP chain applies. **This is the real DJ engine.**
//! * [`StreamingSource`] — a control-only handle to a track playing inside a service
//!   SDK in the webview (Spotify / Apple Music / SoundCloud). It exposes metadata and
//!   capabilities but **never** yields PCM, so no DSP, sync, or true mixing is possible.
//!
//! The two are not interchangeable at the sample level, and the type system reflects
//! that: only [`LocalFileSource`] implements [`PcmSource`].

#![forbid(unsafe_code)]

mod local;
mod streaming;

pub use local::LocalFileSource;
pub use streaming::StreamingSource;

use compas_core::{SourceCapabilities, TrackMetadata};

/// Common to every source: it has metadata and a capability profile.
pub trait AudioSource {
    fn metadata(&self) -> &TrackMetadata;
    fn capabilities(&self) -> SourceCapabilities;
}

/// Implemented only by sources that hand us decoded PCM. The audio engine binds
/// against this trait; a [`StreamingSource`] cannot satisfy it, which is exactly the
/// guarantee we want — you cannot accidentally route a Spotify deck into the DSP graph.
pub trait PcmSource: AudioSource {
    /// Sample rate of the decoded stream, in Hz.
    fn sample_rate(&self) -> u32;
    /// Channel count of the decoded stream.
    fn channels(&self) -> u16;
    /// Decode the next chunk as **interleaved stereo f32**, or `Ok(None)` at EOF.
    ///
    /// Runs on a decoder worker thread (allocates) — never on the audio callback.
    fn next_chunk(&mut self) -> compas_core::Result<Option<Vec<f32>>>;
}
