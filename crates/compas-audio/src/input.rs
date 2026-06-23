//! Auxiliary input capture (microphone / line-in): a cpal **input** stream on a device of the
//! user's choosing that pushes captured audio into a ring the [`Mixer`](crate::mixer::Mixer)
//! drains and sums into the master bus. The inverse of [`cue`](crate::cue): there a 2nd output
//! stream *drains* a ring the mixer fills; here an input stream *fills* a ring the mixer drains.
//!
//! The input stream and the master output stream run on independent device clocks, so the ring
//! slowly drifts. As with the cue/booth monitors we tolerate that the cheap way: the mixer prime-
//! buffers a little audio and outputs silence on underrun (see `Mixer::next_frame`). On overflow
//! (input faster than the master consumes) we drop the newest frame rather than block the capture
//! callback. `cpal::Stream` is `!Send`, so the returned [`AuxInput`] must stay on its thread.

use compas_core::{CompasError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use rtrb::Producer;

/// List the names of available input devices (the first is typically the system default).
/// Names are what [`open_aux_input`] matches against.
pub fn input_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    names
}

/// Owns the aux input stream. Dropping it stops capture.
pub struct AuxInput {
    _stream: cpal::Stream,
    /// The device name actually opened (for logging / UI confirmation).
    pub device_name: String,
}

/// Open the named input device (or the default when `None`) and start pushing captured audio
/// (interleaved stereo f32) into `producer`. A mono device is duplicated to both channels; a
/// device with more than two channels contributes only its first two. Must be called on, and
/// the result kept on, a dedicated thread (`cpal::Stream` is not `Send`).
pub fn open_aux_input(device_name: Option<&str>, producer: Producer<f32>) -> Result<AuxInput> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(want) => host
            .input_devices()
            .map_err(|e| CompasError::Device(e.to_string()))?
            .find(|d| d.name().map(|n| n == want).unwrap_or(false))
            .or_else(|| host.default_input_device())
            .ok_or_else(|| CompasError::Device(format!("aux input device '{want}' not found")))?,
        None => host
            .default_input_device()
            .ok_or_else(|| CompasError::Device("no default input device".into()))?,
    };
    let name = device.name().unwrap_or_else(|_| "aux input".to_string());
    let supported = device
        .default_input_config()
        .map_err(|e| CompasError::Device(e.to_string()))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &config, producer),
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &config, producer),
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &config, producer),
        other => Err(CompasError::Device(format!(
            "unsupported sample format: {other:?}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(AuxInput {
        _stream: stream,
        device_name: name,
    })
}

/// Build the aux input stream for sample type `T`, converting each captured frame to a stereo
/// f32 pair and pushing it into `producer`. Drops on overflow so the capture callback never
/// blocks (RT-safe on the capture side too).
fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: Producer<f32>,
) -> Result<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let err_fn = |e| tracing::error!("aux input stream error: {e}");

    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if channels == 0 {
                    return;
                }
                for frame in data.chunks(channels) {
                    let l = f32::from_sample(frame[0]);
                    // Mono → duplicate; stereo+ → take the first two channels.
                    let r = if channels >= 2 {
                        f32::from_sample(frame[1])
                    } else {
                        l
                    };
                    // Push the pair atomically; drop it whole on overflow so L/R never split.
                    if producer.slots() >= 2 {
                        let _ = producer.push(l);
                        let _ = producer.push(r);
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| CompasError::Device(e.to_string()))?;
    Ok(stream)
}
