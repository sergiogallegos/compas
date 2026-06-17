use compas_core::{MusicProvider, SourceCapabilities, TrackMetadata};

use crate::AudioSource;

/// A control-only handle to a track playing inside a streaming service's SDK
/// **in the webview** (Spotify Web Playback SDK / Apple MusicKit JS / SoundCloud).
///
/// IMPORTANT: This type deliberately does NOT implement [`crate::PcmSource`]. We never
/// receive decoded audio from these services, so DSP, beat-sync, scratching, EQ, and
/// true (single-bus) crossfading are impossible. The actual transport (play/pause/
/// seek/volume) is driven from the frontend via Tauri commands → JS SDK calls; this
/// Rust struct exists so the engine/library layer can reason about the deck uniformly
/// (metadata, capability gating) without pretending it owns the audio.
///
/// See ARCHITECTURE.md §"Two output paths" for why streaming audio cannot be summed
/// into the cpal bus.
pub struct StreamingSource {
    metadata: TrackMetadata,
}

impl StreamingSource {
    pub fn new(metadata: TrackMetadata) -> Self {
        debug_assert!(
            metadata.provider != MusicProvider::Local,
            "StreamingSource must not wrap a local track"
        );
        StreamingSource { metadata }
    }

    pub fn provider(&self) -> MusicProvider {
        self.metadata.provider
    }
}

impl AudioSource for StreamingSource {
    fn metadata(&self) -> &TrackMetadata {
        &self.metadata
    }

    fn capabilities(&self) -> SourceCapabilities {
        // Apple Music is the most restricted (DRM, MusicKit). Spotify/SoundCloud are
        // also control-only from our engine's perspective. All map to PLAYBACK_ONLY.
        SourceCapabilities::PLAYBACK_ONLY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(provider: MusicProvider) -> TrackMetadata {
        TrackMetadata {
            id: "x".into(),
            provider,
            provider_id: "abc".into(),
            title: "t".into(),
            artist: "a".into(),
            album: None,
            artwork_url: None,
            duration_ms: None,
            bpm: None,
            musical_key: None,
        }
    }

    #[test]
    fn streaming_is_playback_only() {
        let s = StreamingSource::new(meta(MusicProvider::Spotify));
        let caps = s.capabilities();
        assert!(!caps.full_dsp);
        assert!(!caps.provides_pcm);
    }
}
