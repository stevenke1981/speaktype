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

/// Microphone recorder.
///
/// When `keep_warm` is enabled the input stream stays open between dictations
/// and continuously fills a short pre-roll ring buffer. Pressing the hotkey then
/// costs nothing (no device open, which takes 100-400 ms on Windows WASAPI) and
/// the first syllable spoken slightly before the key press is not lost.
pub struct Recorder {
    host: cpal::Host,
    input_device: Option<cpal::Device>,
    input_device_name: Option<String>,
    stream: Option<cpal::Stream>,
    shared: Arc<Mutex<CaptureBuffer>>,
    sample_rate: u32,
    channels: u16,
    gain: Arc<Mutex<f32>>,
    keep_warm: bool,
    capturing: bool,
}

struct CaptureBuffer {
    capturing: bool,
    samples: Vec<f32>,
    preroll: std::collections::VecDeque<f32>,
    preroll_capacity: usize,
}

impl CaptureBuffer {
    fn push(&mut self, data: impl Iterator<Item = f32>) {
        if self.capturing {
            self.samples.extend(data);
        } else if self.preroll_capacity > 0 {
            for sample in data {
                if self.preroll.len() == self.preroll_capacity {
                    self.preroll.pop_front();
                }
                self.preroll.push_back(sample);
            }
        }
    }
}

/// Pre-roll kept before the key press.
pub const PREROLL_MS: u32 = 300;

impl Recorder {
    /// Cheap: the audio device is looked up lazily on first use (device
    /// enumeration costs tens of ms on Windows and must not delay startup).
    pub fn new(input_device_name: Option<String>, gain: f32) -> Self {
        let host = cpal::default_host();
        Self {
            host,
            input_device: None,
            input_device_name,
            stream: None,
            shared: Arc::new(Mutex::new(CaptureBuffer {
                capturing: false,
                samples: Vec::new(),
                preroll: Default::default(),
                preroll_capacity: 0,
            })),
            sample_rate: 16_000,
            channels: 1,
            gain: Arc::new(Mutex::new(gain.max(0.1))),
            keep_warm: false,
            capturing: false,
        }
    }

    pub fn list_devices(&self) -> Vec<String> {
        self.host
            .input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    pub fn set_input_device_name(&mut self, input_device_name: Option<String>) {
        if self.input_device_name == input_device_name {
            return;
        }
        self.input_device_name = input_device_name;
        self.input_device = None;
        if !self.capturing {
            self.stream = None;
            if self.keep_warm {
                if let Err(err) = self.ensure_stream() {
                    log_error("audio warm stream", err);
                }
            }
        }
    }

    pub fn set_gain(&mut self, gain: f32) {
        if let Ok(mut g) = self.gain.lock() {
            *g = gain.max(0.1);
        }
    }

    /// Keep the input stream open between recordings (lower start latency).
    pub fn set_keep_warm(&mut self, keep_warm: bool) {
        self.keep_warm = keep_warm;
        if keep_warm {
            if let Err(err) = self.ensure_stream() {
                log_error("audio warm stream", err);
            }
        } else if !self.capturing {
            self.stream = None;
        }
    }

    pub fn is_warm(&self) -> bool {
        self.stream.is_some() && !self.capturing
    }

    fn ensure_stream(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Ok(());
        }
        if self.input_device.is_none() {
            self.input_device = find_input_device(&self.host, self.input_device_name.as_deref());
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
        if let Ok(mut shared) = self.shared.lock() {
            shared.preroll_capacity = if self.keep_warm {
                (self.sample_rate * PREROLL_MS / 1000) as usize * self.channels as usize
            } else {
                0
            };
            shared.preroll.clear();
        }

        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                self.build_stream::<f32>(device, &stream_config, |s| s)?
            }
            cpal::SampleFormat::I16 => {
                self.build_stream::<i16>(device, &stream_config, |s| s as f32 / i16::MAX as f32)?
            }
            cpal::SampleFormat::U16 => self.build_stream::<u16>(device, &stream_config, |s| {
                (s as f32 / u16::MAX as f32) * 2.0 - 1.0
            })?,
            cpal::SampleFormat::I32 => {
                self.build_stream::<i32>(device, &stream_config, |s| s as f32 / i32::MAX as f32)?
            }
            other => return Err(format!("不支援的音訊格式: {other:?}")),
        };
        stream
            .play()
            .map_err(|e| format!("無法開始錄音: {}", e))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build_stream<T: cpal::SizedSample + Send + 'static>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        convert: fn(T) -> f32,
    ) -> Result<cpal::Stream, String> {
        let shared = self.shared.clone();
        let gain = self.gain.clone();
        device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let gain = gain.lock().map(|g| *g).unwrap_or(1.0);
                    match shared.lock() {
                        Ok(mut buf) => buf.push(
                            data.iter()
                                .map(|s| (convert(*s) * gain).clamp(-1.0, 1.0)),
                        ),
                        Err(err) => log_error("audio buffer lock", err),
                    }
                },
                |err| log_error("audio stream", err),
                None,
            )
            .map_err(|e| format!("無法建立音訊串流: {}", e))
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if self.capturing {
            return Ok(());
        }
        self.ensure_stream()?;
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let preroll: Vec<f32> = shared.preroll.drain(..).collect();
        shared.samples.clear();
        shared.samples.extend(preroll);
        shared.capturing = true;
        self.capturing = true;
        Ok(())
    }

    /// Stop capturing and return trimmed 16 kHz mono audio ready for whisper.
    pub fn stop_recording(&mut self) -> RecordedAudio {
        let data = {
            let mut shared = self
                .shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            shared.capturing = false;
            std::mem::take(&mut shared.samples)
        };
        self.capturing = false;
        if !self.keep_warm {
            self.stream = None;
        }
        prepare_for_whisper(&RecordedAudio {
            samples: data,
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.capturing
    }
}

/// Downmix + resample to 16 kHz mono, normalise and trim leading/trailing silence.
///
/// Doing this once, at 16 kHz, before VAD fixes a bug where the energy VAD
/// (which assumes 16 kHz mono frames) was run on interleaved 48 kHz stereo.
pub fn prepare_for_whisper(audio: &RecordedAudio) -> RecordedAudio {
    let mut samples = audio.to_whisper_mono_16khz();
    normalize_audio(&mut samples);
    let mut vad = vad::EnergyVad::default();
    vad.trim_silence_padded(&mut samples, 16_000 / 5);
    RecordedAudio {
        samples,
        sample_rate: 16_000,
        channels: 1,
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
    let gain = (target_rms / rms).clamp(0.25, 4.0);

    if (gain - 1.0).abs() > 0.05 {
        for sample in samples.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}
