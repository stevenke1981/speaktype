use crate::modules::error::log_error;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl RecordedAudio {
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn to_whisper_mono_16khz(&self) -> Vec<f32> {
        let mono = self.to_mono();
        if self.sample_rate == 16_000 {
            return mono;
        }
        resample_linear(&mono, self.sample_rate, 16_000)
    }

    fn to_mono(&self) -> Vec<f32> {
        let channels = self.channels.max(1) as usize;
        if channels == 1 {
            return self.samples.clone();
        }

        self.samples
            .chunks(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
            .collect()
    }
}

pub struct Recorder {
    host: cpal::Host,
    input_device: Option<cpal::Device>,
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl Recorder {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let input_device = host.default_input_device();
        Self {
            host,
            input_device,
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 16_000,
            channels: 1,
        }
    }

    pub fn list_devices(&self) -> Vec<String> {
        self.host
            .input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }

        let device = self
            .input_device
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;

        let config = device.default_input_config()?;
        self.sample_rate = config.sample_rate().0;
        self.channels = config.channels();
        let buffer = self.buffer.clone();
        let err_fn = |err| log_error("audio stream", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let stream = device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| match buffer.lock() {
                        Ok(mut buf) => buf.extend_from_slice(data),
                        Err(err) => log_error("audio buffer lock", err),
                    },
                    err_fn,
                    None,
                )?;
                stream
            }
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| match buffer.lock() {
                    Ok(mut buf) => {
                        buf.extend(data.iter().map(|sample| *sample as f32 / i16::MAX as f32));
                    }
                    Err(err) => log_error("audio buffer lock", err),
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| match buffer.lock() {
                    Ok(mut buf) => {
                        buf.extend(data.iter().map(|sample| {
                            ((*sample as f32 / u16::MAX as f32) * 2.0 - 1.0).clamp(-1.0, 1.0)
                        }));
                    }
                    Err(err) => log_error("audio buffer lock", err),
                },
                err_fn,
                None,
            )?,
            sample_format => {
                return Err(anyhow::anyhow!(
                    "Unsupported sample format: {sample_format:?}"
                ))
            }
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> RecordedAudio {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        let mut buf = match self.buffer.lock() {
            Ok(buf) => buf,
            Err(err) => {
                log_error("audio buffer recover", &err);
                err.into_inner()
            }
        };
        let data = buf.clone();
        buf.clear();
        RecordedAudio {
            samples: data,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || from_rate == to_rate {
        return samples.to_vec();
    }

    let output_len = (samples.len() as u64 * to_rate as u64 / from_rate as u64).max(1) as usize;
    let ratio = from_rate as f64 / to_rate as f64;
    let mut output = Vec::with_capacity(output_len);

    for out_idx in 0..output_len {
        let src_pos = out_idx as f64 * ratio;
        let left = src_pos.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let frac = (src_pos - left as f64) as f32;
        output.push(samples[left] * (1.0 - frac) + samples[right] * frac);
    }

    output
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}
