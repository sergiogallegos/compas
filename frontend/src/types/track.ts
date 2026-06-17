// Mirrors `compas_core::TrackMetadata` / `SourceCapabilities`. Keep in sync with
// crates/compas-core. (A future step can codegen this from Rust via ts-rs/specta.)

export type MusicProvider = "local" | "spotify" | "apple_music" | "soundcloud";

export interface TrackMetadata {
  id: string;
  provider: MusicProvider;
  provider_id: string;
  title: string;
  artist: string;
  album?: string;
  artwork_url?: string;
  duration_ms?: number;
  bpm?: number;
  musical_key?: string;
}

export interface SourceCapabilities {
  full_dsp: boolean;
  provides_pcm: boolean;
  can_seek: boolean;
  can_vary_tempo: boolean;
}

/** Capabilities implied by the provider — the UI uses this to disable controls
 *  that cannot work for streaming decks (honest degradation, never faked DSP). */
export function capabilitiesFor(provider: MusicProvider): SourceCapabilities {
  if (provider === "local") {
    return { full_dsp: true, provides_pcm: true, can_seek: true, can_vary_tempo: true };
  }
  return { full_dsp: false, provides_pcm: false, can_seek: true, can_vary_tempo: false };
}
