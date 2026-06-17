use crate::modules::error::log_error;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub mod vad;

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
    input_device_name: Option<String>,
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    gain: f32,
}

impl Recorder {
    pub fn new(input_device_name: Option<String>, gain: f32) -> Self {
        let host = cpal::default_host();
        let input_device = find_input_device(&host, input_device_name.as_deref());
        Self {
            host,
            input_device,
            input_device_name,
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 16_000,
            channels: 1,
            gain: gain.max(0.1),
        }
    }

    pub fn list_devices(&self) -> Vec<String> {
        self.host
            .input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    pub fn set_input_device_name(&mut self, input_device_name: Option<String>) {
        self.input_device_name = input_device_name;
        self.input_device = find_input_device(&self.host, self.input_device_name.as_deref());
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.max(0.1);
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Ok(());
        }

        let device = self
            .input_device
            .as_ref()
            .ok_or_else(|| "找不到輸入裝置".to_string())?;

        let config = device
            .default_input_config()
            .map_err(|e| format!("無法取得音訊設定: {}", e))?;
        self.sample_rate = config.sample_rate().0;
        self.channels = config.channels();
        let buffer = self.buffer.clone();
        let gain = self.gain;
        let err_fn = |err| log_error("audio stream", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let stream = device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| match buffer.lock() {
                        Ok(mut buf) => {
                            buf.extend(data.iter().map(|sample| (sample * gain).clamp(-1.0, 1.0)));
                        }
                        Err(err) => log_error("audio buffer lock", err),
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?;
                stream
            }
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| match buffer.lock() {
                        Ok(mut buf) => {
                            buf.extend(data.iter().map(|sample| {
                                ((*sample as f32 / i16::MAX as f32) * gain).clamp(-1.0, 1.0)
                            }));
                        }
                        Err(err) => log_error("audio buffer lock", err),
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?,
            cpal::SampleFormat::U16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| match buffer.lock() {
                        Ok(mut buf) => {
                            buf.extend(data.iter().map(|sample| {
                                (((*sample as f32 / u16::MAX as f32) * 2.0 - 1.0) * gain)
                                    .clamp(-1.0, 1.0)
                            }));
                        }
                        Err(err) => log_error("audio buffer lock", err),
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?,
            sample_format => {
                return Err(format!("不支援的音訊格式: {sample_format:?}"))
            }
        };

        stream
            .play()
            .map_err(|e| format!("無法開始錄音: {}", e))?;
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
        let mut data = buf.clone();
        buf.clear();
        drop(buf);

        normalize_audio(&mut data);
        let mut vad = vad::EnergyVad::default();
        vad.trim_silence(&mut data);

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
        Self::new(None, 1.0)
    }
}

pub struct LevelMonitor {
    stream: Option<cpal::Stream>,
    level: Arc<Mutex<f32>>,
}

impl LevelMonitor {
    pub fn start(input_device_name: Option<String>, gain: f32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = find_input_device(&host, input_device_name.as_deref())
            .ok_or_else(|| "找不到輸入裝置".to_string())?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("無法取得音訊設定: {}", e))?;
        let level = Arc::new(Mutex::new(0.0_f32));
        let level_for_callback = level.clone();
        let gain = gain.max(0.1);
        let err_fn = |err| log_error("audio level stream", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        update_level(
                            &level_for_callback,
                            data.iter().map(|sample| sample * gain),
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?,
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        update_level(
                            &level_for_callback,
                            data.iter()
                                .map(|sample| (*sample as f32 / i16::MAX as f32) * gain),
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?,
            cpal::SampleFormat::U16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        update_level(
                            &level_for_callback,
                            data.iter()
                                .map(|sample| ((*sample as f32 / u16::MAX as f32) * 2.0 - 1.0) * gain),
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("無法建立音訊串流: {}", e))?,
            sample_format => {
                return Err(format!("不支援的音訊格式: {sample_format:?}"))
            }
        };

        stream
            .play()
            .map_err(|e| format!("無法開始錄音: {}", e))?;
        Ok(Self {
            stream: Some(stream),
            level,
        })
    }

    pub fn level(&self) -> f32 {
        self.level.lock().map(|level| *level).unwrap_or(0.0)
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
    }
}

impl Drop for LevelMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_input_device(host: &cpal::Host, input_device_name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = input_device_name.filter(|name| !name.trim().is_empty()) {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(device) = devices.find(|device| {
                device
                    .name()
                    .map(|device_name| device_name == name)
                    .unwrap_or(false)
            }) {
                return Some(device);
            }
        }
    }

    host.default_input_device()
}

fn update_level(level: &Arc<Mutex<f32>>, samples: impl Iterator<Item = f32>) {
    let peak = samples
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max)
        .clamp(0.0, 1.0);

    if let Ok(mut current) = level.lock() {
        *current = (*current * 0.65).max(peak);
    }
}

fn normalize_audio(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }

    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 0.001 {
        return;
    }

    let target_rms = 0.25;
    let gain = (target_rms / rms).min(4.0).max(0.25);

    if (gain - 1.0).abs() > 0.05 {
        for sample in samples.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}
