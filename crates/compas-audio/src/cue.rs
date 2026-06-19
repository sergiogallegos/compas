//! The headphone / cue-monitor output: a *second* cpal output stream, on a device of the
//! user's choosing, that simply drains the cue mix the [`Mixer`](crate::mixer::Mixer) pushes
//! into a ring. Keeping all DSP + the play-heads in the one master mixer avoids double-
//! advancing decks; this stream is a dumb consumer.
//!
//! The two streams run on independent device clocks, so the ring slowly drifts. We tolerate
//! that the cheap way (this is monitoring, not the master): prime a small latency buffer
//! before draining, and on underrun output silence and re-prime. `cpal::Stream` is `!Send`,
//! so the returned [`CueOutput`] must stay on the thread that created it.

use compas_core::{CompasError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use rtrb::Consumer;

/// Target buffered latency before the cue stream starts draining (and the level it re-primes
/// to after an underrun). ~21 ms of stereo @ 48 kHz — small, but enough to ride clock drift.
const PRIME_SAMPLES: usize = 2048;

/// List the names of available output devices (the first is typically the system default).
/// Names are what [`open_cue_output`] matches against.
pub fn output_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    names
}

/// Owns the cue output stream. Dropping it stops the stream.
pub struct CueOutput {
    _stream: cpal::Stream,
    /// The device name actually opened (for logging / UI confirmation).
    pub device_name: String,
}

/// Open the named output device (or the default when `None`) and start draining `consumer`
/// — the cue mix the mixer pushes (interleaved stereo f32). Must be called on, and the result
/// kept on, a dedicated thread (`cpal::Stream` is not `Send`).
pub fn open_cue_output(device_name: Option<&str>, consumer: Consumer<f32>) -> Result<CueOutput> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(want) => host
            .output_devices()
            .map_err(|e| CompasError::Device(e.to_string()))?
            .find(|d| d.name().map(|n| n == want).unwrap_or(false))
            .or_else(|| host.default_output_device())
            .ok_or_else(|| CompasError::Device(format!("cue output device '{want}' not found")))?,
        None => host
            .default_output_device()
            .ok_or_else(|| CompasError::Device("no default output device".into()))?,
    };
    let name = device.name().unwrap_or_else(|_| "cue output".into());
    let supported = device
        .default_output_config()
        .map_err(|e| CompasError::Device(e.to_string()))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_cue_stream::<f32>(&device, &config, consumer),
        cpal::SampleFormat::I16 => build_cue_stream::<i16>(&device, &config, consumer),
        cpal::SampleFormat::U16 => build_cue_stream::<u16>(&device, &config, consumer),
        other => Err(CompasError::Device(format!(
            "unsupported sample format: {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(CueOutput {
        _stream: stream,
        device_name: name,
    })
}

/// Build the cue output stream for sample type `T`, draining `consumer` with prime/underrun
/// handling so a same-rate producer/consumer pair doesn't glitch on every block.
fn build_cue_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut consumer: Consumer<f32>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let err_fn = |e| tracing::error!("cue stream error: {e}");
    // Wait for PRIME_SAMPLES of buffered audio before draining; re-arm after an underrun.
    let mut primed = false;

    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !primed {
                    if consumer.slots() >= PRIME_SAMPLES {
                        primed = true;
                    } else {
                        for s in out.iter_mut() {
                            *s = T::from_sample(0.0f32);
                        }
                        return;
                    }
                }
                for frame in out.chunks_mut(channels) {
                    // Pop a stereo pair atomically; an underrun outputs silence and re-primes.
                    let (l, r) = if consumer.slots() >= 2 {
                        (
                            consumer.pop().unwrap_or(0.0),
                            consumer.pop().unwrap_or(0.0),
                        )
                    } else {
                        primed = false;
                        (0.0, 0.0)
                    };
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
            },
            err_fn,
            None,
        )
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(stream)
}
