use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use compas_core::{CompasError, DeckBuffer, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::mixer::{AudioCommand, DeckTelemetry, Mixer};

/// Engine configuration.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    /// Capacity of the control-command ring (commands per audio block are tiny).
    pub command_capacity: usize,
    /// Capacity of the buffer-reclaim ring (retired decks awaiting drop on control thread).
    pub reclaim_capacity: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            command_capacity: 256,
            reclaim_capacity: 16,
        }
    }
}

/// Owns the cpal output stream and the control-side command producer. Dropping the
/// engine stops the stream. Created and owned on a dedicated audio thread because
/// `cpal::Stream` is not `Send` on all platforms.
pub struct AudioEngine {
    _stream: cpal::Stream,
    commands: Producer<AudioCommand>,
    reclaim: Consumer<Arc<DeckBuffer>>,
    telemetry: Arc<DeckTelemetry>,
    sample_rate: u32,
    stream_failed: Arc<AtomicBool>,
    last_stream_error: Arc<Mutex<Option<String>>>,
}

impl AudioEngine {
    /// Open the default output device and start the audio stream. `telemetry` is shared
    /// so other threads can read deck position/state without locking.
    pub fn new(config: EngineConfig, telemetry: Arc<DeckTelemetry>) -> Result<Self> {
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
        let (reclaim_tx, reclaim_rx) = RingBuffer::<Arc<DeckBuffer>>::new(config.reclaim_capacity);
        let mixer = Mixer::new(sample_rate as f32, cmd_rx, reclaim_tx, telemetry.clone());
        let stream_failed = Arc::new(AtomicBool::new(false));
        let last_stream_error = Arc::new(Mutex::new(None));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(
                &device,
                &stream_config,
                mixer,
                stream_failed.clone(),
                last_stream_error.clone(),
            ),
            cpal::SampleFormat::I16 => build_stream::<i16>(
                &device,
                &stream_config,
                mixer,
                stream_failed.clone(),
                last_stream_error.clone(),
            ),
            cpal::SampleFormat::U16 => build_stream::<u16>(
                &device,
                &stream_config,
                mixer,
                stream_failed.clone(),
                last_stream_error.clone(),
            ),
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
            reclaim: reclaim_rx,
            telemetry,
            sample_rate,
            stream_failed,
            last_stream_error,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn telemetry(&self) -> Arc<DeckTelemetry> {
        self.telemetry.clone()
    }

    /// Whether cpal has reported a stream/device error after startup. This is set from cpal's
    /// error callback (not the audio data callback) so the owner thread can rebuild the stream.
    pub fn stream_failed(&self) -> bool {
        self.stream_failed.load(Ordering::Relaxed)
    }

    /// Last stream/device error text, if cpal provided one.
    pub fn last_stream_error(&self) -> Option<String> {
        self.last_stream_error.lock().ok().and_then(|e| e.clone())
    }

    /// Drop any buffers the audio thread has retired (call periodically / after sends so
    /// freeing never happens on the RT path).
    pub fn drain_reclaimed(&mut self) {
        while self.reclaim.pop().is_ok() {}
    }

    /// Send a control command to the audio thread. Errors if the ring is full (the UI
    /// should coalesce rapid parameter changes).
    pub fn send(&mut self, cmd: AudioCommand) -> Result<()> {
        let r = self
            .commands
            .push(cmd)
            .map_err(|_| CompasError::Other("audio command ring full".into()));
        self.drain_reclaimed();
        r
    }
}

/// Build a cpal output stream for sample type `T`, driving it from `mixer`.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: Mixer,
    stream_failed: Arc<AtomicBool>,
    last_stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f64;
    let err_fn = move |e: cpal::StreamError| {
        let message = e.to_string();
        stream_failed.store(true, Ordering::Relaxed);
        if let Ok(mut last) = last_stream_error.lock() {
            *last = Some(message.clone());
        }
        tracing::error!("cpal stream error: {message}");
    };

    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [T], info: &cpal::OutputCallbackInfo| {
                let t0 = std::time::Instant::now();
                // Measured DAC latency: time from this callback to when the audio is heard.
                let ts = info.timestamp();
                if let Some(d) = ts.playback.duration_since(&ts.callback) {
                    mixer.publish_latency(d.as_secs_f32());
                }
                mixer.drain_commands();
                for frame in out.chunks_mut(channels) {
                    let (l, r) = mixer.next_frame();
                    match channels {
                        1 => frame[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            frame[0] = T::from_sample(l);
                            frame[1] = T::from_sample(r);
                            for ch in frame.iter_mut().skip(2) {
                                *ch = T::from_sample(0.0f32);
                            }
                        }
                    }
                }
                // RT load = time spent in the callback ÷ this block's real-time budget.
                let block_secs = (out.len() / channels) as f64 / sample_rate;
                let load = if block_secs > 0.0 {
                    (t0.elapsed().as_secs_f64() / block_secs) as f32
                } else {
                    0.0
                };
                mixer.publish_rt_load(load);
                mixer.publish_telemetry();
            },
            err_fn,
            None,
        )
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(stream)
}
