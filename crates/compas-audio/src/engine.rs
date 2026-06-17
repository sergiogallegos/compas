use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use compas_core::{CompasError, Result};
use rtrb::{Producer, RingBuffer};

use crate::mixer::{AudioCommand, Mixer};

/// Producer end of a deck's PCM ring. A decoder thread writes interleaved stereo
/// f32 frames here; the audio callback consumes them. Single-producer/single-consumer.
pub struct DeckPcmProducer {
    pub deck: usize,
    inner: Producer<f32>,
}

impl DeckPcmProducer {
    /// Push as many interleaved samples as fit; returns the number actually written.
    /// Non-blocking and safe to call from a decoder worker. (Not the RT thread, but
    /// still allocation-free.)
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let mut n = 0;
        for &s in samples {
            if self.inner.push(s).is_err() {
                break;
            }
            n += 1;
        }
        n
    }

    /// Free space in samples.
    pub fn slots(&self) -> usize {
        self.inner.slots()
    }
}

/// Engine configuration. Defaults aim at a safe first-run, not minimum latency.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    /// Capacity (in stereo *frames*) of each per-deck PCM ring. Larger = more
    /// resilient to decode jitter, but more latency between scratch/seek and sound.
    pub deck_ring_frames: usize,
    /// Capacity of the control-command ring.
    pub command_capacity: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            deck_ring_frames: 1 << 15, // 32768 frames ≈ 0.68 s @ 48 kHz
            command_capacity: 256,
        }
    }
}

/// Owns the cpal output stream and the control-side command producer.
///
/// Dropping the engine stops the stream.
pub struct AudioEngine {
    _stream: cpal::Stream,
    commands: Producer<AudioCommand>,
    sample_rate: u32,
    config: EngineConfig,
}

impl AudioEngine {
    /// Open the default output device and start the audio stream.
    pub fn new(config: EngineConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| CompasError::Device("no default output device".into()))?;
        let supported = device
            .default_output_config()
            .map_err(|e| CompasError::Device(e.to_string()))?;

        let sample_rate = supported.sample_rate().0;
        let sample_format = supported.sample_format();
        let stream_config: cpal::StreamConfig = supported.into();

        let (cmd_tx, cmd_rx) = RingBuffer::<AudioCommand>::new(config.command_capacity);
        let mixer = Mixer::new(sample_rate as f32, cmd_rx);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, mixer),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, mixer),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, mixer),
            other => Err(CompasError::Device(format!(
                "unsupported sample format: {other:?}"
            ))),
        }?;

        stream
            .play()
            .map_err(|e| CompasError::Device(e.to_string()))?;

        Ok(AudioEngine {
            _stream: stream,
            commands: cmd_tx,
            sample_rate,
            config,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Send a control command to the audio thread. Returns an error if the command
    /// ring is full (the UI should coalesce rapid parameter changes).
    pub fn send(&mut self, cmd: AudioCommand) -> Result<()> {
        self.commands
            .push(cmd)
            .map_err(|_| CompasError::Other("audio command ring full".into()))
    }

    /// Create a fresh PCM ring for `deck`, hand the consumer to the audio thread,
    /// and return the producer for a decoder to fill.
    pub fn attach_deck(&mut self, deck: usize) -> Result<DeckPcmProducer> {
        // Stereo interleaved: 2 samples per frame.
        let capacity = self.config.deck_ring_frames * 2;
        let (tx, rx) = RingBuffer::<f32>::new(capacity);
        self.send(AudioCommand::AttachDeck { deck, pcm: rx })?;
        Ok(DeckPcmProducer { deck, inner: tx })
    }
}

/// Build a cpal output stream for sample type `T`, driving it from `mixer`.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: Mixer,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let err_fn = |e| tracing::error!("cpal stream error: {e}");

    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                // 1) apply pending control changes (lock-free)
                mixer.drain_commands();
                // 2) render frame by frame (allocation-free)
                for frame in out.chunks_mut(channels) {
                    let (l, r) = mixer.next_frame();
                    match channels {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            // Zero any extra channels (surround) for now.
                            for ch in frame.iter_mut().skip(2) {
                                *ch = T::from_sample(0.0f32);
                            }
                        }
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(stream)
}

// Keep an explicit reference so clippy doesn't flag the trait import as unused on
// platforms where `Sample` is only needed transitively.
#[allow(dead_code)]
fn _assert_sample_bound<T: Sample>() {}
