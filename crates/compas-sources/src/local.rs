use std::fs::File;
use std::path::{Path, PathBuf};

use compas_core::{CompasError, MusicProvider, Result, SourceCapabilities, TrackMetadata};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{AudioSource, PcmSource};

/// A local DRM-free audio file decoded to PCM. Full DSP applies.
pub struct LocalFileSource {
    metadata: TrackMetadata,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
}

impl LocalFileSource {
    /// Open and probe a file, preparing a decoder for its default audio track.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|e| CompasError::UnsupportedFormat(e.to_string()))?;

        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| CompasError::Decode("no decodable audio track".into()))?;

        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| CompasError::Decode("unknown sample rate".into()))?;
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let duration_ms = match (track.codec_params.n_frames, sample_rate) {
            (Some(frames), sr) if sr > 0 => Some(frames * 1000 / sr as u64),
            _ => None,
        };

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| CompasError::Decode(e.to_string()))?;

        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let metadata = TrackMetadata {
            id: title.clone(), // TODO(P1): assign a real UUID at import time.
            provider: MusicProvider::Local,
            provider_id: path.to_string_lossy().into_owned(),
            title,
            artist: "Unknown".to_string(),
            album: None,
            artwork_url: None,
            duration_ms,
            bpm: None,
            musical_key: None,
        };

        Ok(LocalFileSource {
            metadata,
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
        })
    }
}

impl AudioSource for LocalFileSource {
    fn metadata(&self) -> &TrackMetadata {
        &self.metadata
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities::FULL_DSP
    }
}

impl PcmSource for LocalFileSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn next_chunk(&mut self) -> Result<Option<Vec<f32>>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                // Clean EOF.
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                // Stream told us to reset; for the scaffold we treat it as end-of-stream.
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(CompasError::Decode(e.to_string())),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    buf.copy_interleaved_ref(decoded);
                    let src_channels = spec.channels.count();
                    let interleaved = to_stereo(buf.samples(), src_channels);
                    return Ok(Some(interleaved));
                }
                // Recoverable decode errors: skip this packet, keep going.
                Err(SymphoniaError::DecodeError(msg)) => {
                    tracing::warn!("recoverable decode error: {msg}");
                    continue;
                }
                Err(e) => return Err(CompasError::Decode(e.to_string())),
            }
        }
    }
}

/// Convert an interleaved buffer with `src_channels` channels to interleaved stereo.
fn to_stereo(samples: &[f32], src_channels: usize) -> Vec<f32> {
    match src_channels {
        0 => Vec::new(),
        1 => {
            let mut out = Vec::with_capacity(samples.len() * 2);
            for &s in samples {
                out.push(s);
                out.push(s);
            }
            out
        }
        2 => samples.to_vec(),
        n => {
            // Downmix: take the first two channels of each frame.
            let frames = samples.len() / n;
            let mut out = Vec::with_capacity(frames * 2);
            for f in 0..frames {
                out.push(samples[f * n]);
                out.push(samples[f * n + 1]);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_stereo_duplicates_mono() {
        let mono = [0.1, 0.2, 0.3];
        let st = to_stereo(&mono, 1);
        assert_eq!(st, vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3]);
    }

    #[test]
    fn to_stereo_passes_stereo() {
        let s = [0.1, 0.2, 0.3, 0.4];
        assert_eq!(to_stereo(&s, 2), s.to_vec());
    }

    #[test]
    fn to_stereo_downmixes_surround() {
        // 4-channel frame -> keep first two.
        let s = [1.0, 2.0, 9.0, 9.0, 3.0, 4.0, 9.0, 9.0];
        assert_eq!(to_stereo(&s, 4), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn open_missing_file_errors() {
        assert!(LocalFileSource::open("does-not-exist.wav").is_err());
    }
}
