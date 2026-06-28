const DEFAULT_FRAME_MS: u32 = 30;
const TARGET_SAMPLE_RATE: u32 = 16_000;

pub enum VadFrame<'a> {
    Speech(&'a [f32]),
    Noise,
}

impl<'a> VadFrame<'a> {
    pub fn is_speech(&self) -> bool {
        matches!(self, VadFrame::Speech(_))
    }
}

pub trait VoiceActivityDetector: Send {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> VadFrame<'a>;
    fn reset(&mut self);
}

pub struct EnergyVad {
    threshold: f32,
    min_frames_for_speech: u32,
    silence_frames_for_end: u32,
    frame_samples: usize,
    consecutive_speech: u32,
    consecutive_silence: u32,
}

impl EnergyVad {
    pub fn new(
        threshold: f32,
        min_speech_ms: u32,
        min_silence_ms: u32,
    ) -> Self {
        let frame_samples = (TARGET_SAMPLE_RATE * DEFAULT_FRAME_MS / 1000) as usize;
        let min_frames_for_speech = min_speech_ms / DEFAULT_FRAME_MS;
        let silence_frames_for_end = min_silence_ms / DEFAULT_FRAME_MS;

        Self {
            threshold,
            min_frames_for_speech: min_frames_for_speech.max(1),
            silence_frames_for_end: silence_frames_for_end.max(1),
            frame_samples,
            consecutive_speech: 0,
            consecutive_silence: 0,
        }
    }

    fn frame_rms(frame: &[f32]) -> f32 {
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        (sum_sq / frame.len() as f32).sqrt()
    }

    pub fn detect_speech_segments(&mut self, samples: &[f32]) -> Vec<(usize, usize)> {
        let mut segments = Vec::new();
        let mut in_speech = false;
        let mut speech_start = 0;

        for (i, chunk) in samples.chunks(self.frame_samples).enumerate() {
            if chunk.len() < self.frame_samples / 2 {
                break;
            }

            let rms = Self::frame_rms(chunk);
            let is_active = rms > self.threshold;

            if is_active {
                self.consecutive_speech += 1;
                self.consecutive_silence = 0;

                if !in_speech && self.consecutive_speech >= self.min_frames_for_speech {
                    in_speech = true;
                    speech_start = i.saturating_sub(self.min_frames_for_speech as usize - 1) * self.frame_samples;
                }
            } else {
                self.consecutive_silence += 1;
                self.consecutive_speech = 0;

                if in_speech && self.consecutive_silence >= self.silence_frames_for_end {
                    let end = i.saturating_sub(self.silence_frames_for_end as usize - 1) * self.frame_samples;
                    segments.push((speech_start, end.min(samples.len())));
                    in_speech = false;
                }
            }
        }

        if in_speech {
            segments.push((speech_start, samples.len()));
        }

        segments
    }

    pub fn trim_silence(&mut self, samples: &mut Vec<f32>) {
        let segments = self.detect_speech_segments(samples);
        if segments.is_empty() {
            samples.clear();
            return;
        }

        let start = segments.first().map(|s| s.0).unwrap_or(0);
        let end = segments.last().map(|s| s.1).unwrap_or(samples.len());

        if start > 0 || end < samples.len() {
            let trimmed = samples[start..end].to_vec();
            *samples = trimmed;
        }
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(0.02, 60, 300)
    }
}

impl VoiceActivityDetector for EnergyVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> VadFrame<'a> {
        let rms = Self::frame_rms(frame);
        if rms > self.threshold {
            self.consecutive_speech += 1;
            self.consecutive_silence = 0;
            if self.consecutive_speech >= self.min_frames_for_speech {
                VadFrame::Speech(frame)
            } else {
                VadFrame::Noise
            }
        } else {
            self.consecutive_speech = 0;
            self.consecutive_silence += 1;
            VadFrame::Noise
        }
    }

    fn reset(&mut self) {
        self.consecutive_speech = 0;
        self.consecutive_silence = 0;
    }
}

pub fn trim_leading_trailing_silence(samples: &mut Vec<f32>, sample_rate: u32) {
    if samples.is_empty() {
        return;
    }

    let frame_len = (sample_rate / 100) as usize;
    let threshold = 0.015;
    let min_speech_frames = 2;
    let min_silence_frames = 10;

    let frame_rms = |chunk: &[f32]| -> f32 {
        let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
        (sum_sq / chunk.len() as f32).sqrt()
    };

    let total_frames = samples.len() / frame_len;
    let mut frame_flags = Vec::with_capacity(total_frames);

    for i in 0..total_frames {
        let start = i * frame_len;
        let end = (start + frame_len).min(samples.len());
        let rms = frame_rms(&samples[start..end]);
        frame_flags.push(rms > threshold);
    }

    let mut speech_frames = 0;
    let mut in_speech = false;
    let mut first_speech_frame = 0;
    let mut last_speech_frame = 0;

    for (i, &is_speech) in frame_flags.iter().enumerate() {
        if is_speech {
            speech_frames += 1;
            if speech_frames >= min_speech_frames && !in_speech {
                in_speech = true;
                first_speech_frame = i.saturating_sub(min_speech_frames - 1);
            }
        } else {
            speech_frames = 0;
        }

        if in_speech {
            last_speech_frame = i;
        }
    }

    if !in_speech {
        samples.clear();
        return;
    }

    let mut silence_count = 0;
    for i in (first_speech_frame..=last_speech_frame).rev() {
        if !frame_flags[i] {
            silence_count += 1;
        } else {
            silence_count = 0;
        }
        if silence_count >= min_silence_frames {
            last_speech_frame = i + min_silence_frames;
            break;
        }
    }

    let trim_start = first_speech_frame * frame_len;
    let trim_end = (last_speech_frame + 1) * frame_len;

    if trim_start > 0 || trim_end < samples.len() {
        let trimmed = samples[trim_start..trim_end.min(samples.len())].to_vec();
        *samples = trimmed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_energy_vad_noise() {
        let mut vad = EnergyVad::default();
        let frame = vec![0.0_f32; 480];
        assert_eq!(vad.push_frame(&frame).is_speech(), false);
    }

    #[test]
    fn test_energy_vad_speech() {
        let mut vad = EnergyVad::default();
        let frame = vec![0.5_f32; 480];
        assert_eq!(vad.push_frame(&frame).is_speech(), true);
    }

    #[test]
    fn test_trim_silence_empty() {
        let mut samples = Vec::new();
        trim_leading_trailing_silence(&mut samples, 16000);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_trim_silence_all_noise() {
        let mut samples = vec![0.0_f32; 16000];
        trim_leading_trailing_silence(&mut samples, 16000);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_detect_speech_segments() {
        let mut vad = EnergyVad::new(0.02, 60, 300);
        let mut samples = vec![0.0_f32; 16000];
        for i in 2000..4000 {
            samples[i] = 0.5;
        }
        let segments = vad.detect_speech_segments(&samples);
        assert!(!segments.is_empty());
    }
}
