use serde::{Deserialize, Serialize};

/// Where a deck's audio comes from. This is the single most important
/// distinction in the whole application, because it determines whether the
/// audio engine can touch the samples at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Local DRM-free file. Decoded to PCM in-process; full DSP available.
    LocalFile,
    /// A streaming service (Spotify / Apple Music / SoundCloud) played through
    /// its own SDK in the webview. We get **control only** — no PCM, no DSP.
    Streaming,
}

/// What an audio source actually permits. The UI reads this to enable/disable
/// controls honestly rather than faking DSP that cannot work (see ARCHITECTURE.md
/// §"Capability degradation").
///
/// Invariant: `full_dsp` implies `provides_pcm`. A source that does not hand us
/// samples can never be time-stretched, EQ'd, filtered, or beat-synced *in our engine*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCapabilities {
    /// The engine receives decoded PCM and can run the full DSP chain
    /// (gain, EQ, filter, time-stretch, beat-sync, scratch, FX).
    pub full_dsp: bool,
    /// The engine can read sample buffers at all. Always true for local files,
    /// always false for Spotify/Apple Web SDKs.
    pub provides_pcm: bool,
    /// Transport supports seeking to an arbitrary position.
    pub can_seek: bool,
    /// Playback rate can be changed (true varispeed for PCM; limited/none for streaming).
    pub can_vary_tempo: bool,
}

impl SourceCapabilities {
    /// Full local-file DJ deck: everything is possible.
    pub const FULL_DSP: SourceCapabilities = SourceCapabilities {
        full_dsp: true,
        provides_pcm: true,
        can_seek: true,
        can_vary_tempo: true,
    };

    /// Streaming control-only deck: play/pause/seek/volume via the service SDK,
    /// nothing more. No mixing, EQ, filter, sync, or scratch in our engine.
    pub const PLAYBACK_ONLY: SourceCapabilities = SourceCapabilities {
        full_dsp: false,
        provides_pcm: false,
        can_seek: true,
        can_vary_tempo: false,
    };

    pub fn kind(&self) -> SourceKind {
        if self.provides_pcm {
            SourceKind::LocalFile
        } else {
            SourceKind::Streaming
        }
    }
}
