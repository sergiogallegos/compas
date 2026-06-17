use serde::{Deserialize, Serialize};

/// A streaming/metadata provider. `Local` is the first-class DJ path; the others
/// are control-only (see [`crate::SourceCapabilities`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicProvider {
    Local,
    Spotify,
    AppleMusic,
    SoundCloud,
}

/// Normalized track metadata shared across providers and the UI.
///
/// Ported and renamed from djvibebar's `TrackMetadata`. The original conflated
/// every provider id into a field named `spotify_id`; here it is `provider_id`
/// and the owning provider is explicit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackMetadata {
    /// Stable local id (UUID) we assign on import/search.
    pub id: String,
    /// Provider that owns `provider_id` (Spotify track id, SoundCloud id, file path hash, …).
    pub provider: MusicProvider,
    /// The provider-native id. For `Local`, the absolute file path.
    pub provider_id: String,
    pub title: String,
    pub artist: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Detected/known tempo in beats-per-minute. `None` until analyzed.
    /// For streaming decks this is usually permanently `None` (no PCM to analyze,
    /// and Spotify's audio-features endpoint is no longer available to new apps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f32>,
    /// Detected musical key (e.g. "8A" Camelot / "Am"). `None` until analyzed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub musical_key: Option<String>,
}

impl TrackMetadata {
    /// Capabilities implied purely by the provider. Local files get full DSP;
    /// everything else is control-only.
    pub fn capabilities(&self) -> crate::SourceCapabilities {
        match self.provider {
            MusicProvider::Local => crate::SourceCapabilities::FULL_DSP,
            _ => crate::SourceCapabilities::PLAYBACK_ONLY,
        }
    }
}
