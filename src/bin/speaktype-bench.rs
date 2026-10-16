//! Latency benchmark for the post-hotkey pipeline (no microphone needed).
//!
//! usage: speaktype-bench <model.bin> <audio.wav> [runs] [--gpu] [--lang auto|zh|en] [--no-fast]
//!
//! Prints model load / warm-up time and, per run, the time spent in each stage
//! from "hotkey released" to "text handed to the paste/SendInput call".
use speaktype::modules::audio::prepare_for_whisper;
use speaktype::modules::config::{AppConfig, InjectionConfig};
use speaktype::modules::engine::{build_initial_prompt, load_recording_wav};
use speaktype::modules::postprocess::postprocess_transcription;
use speaktype::modules::scenario::Scenario;
use speaktype::modules::transcription::{TranscribeOptions, Transcriber};
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: speaktype-bench <model.bin> <audio.wav> [runs] [--gpu] [--lang L] [--no-fast]");
        std::process::exit(2);
    }
    let model = std::path::PathBuf::from(&args[1]);
    let raw = load_recording_wav(std::path::Path::new(&args[2])).expect("wav");
    let runs: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
    let gpu = args.iter().any(|a| a == "--gpu");
    let mut config = AppConfig::default();
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        config.transcription.language = args[i + 1].clone();
    }
    if let Some(i) = args.iter().position(|a| a == "--threads") {
        config.transcription.threads = args[i + 1].parse().unwrap_or(0);
    }
    if args.iter().any(|a| a == "--no-fast") {
        config.transcription.fast_encoder = false;
    }

    let t = Instant::now();
    let transcriber = Transcriber::new(&model, gpu).expect("model");
    println!("model_load_ms={:.0} backend={}", ms(t), transcriber.backend().label());
    println!("system_info={}", whisper_rs::print_system_info().trim());
    let options = TranscribeOptions {
        language: config.transcription.language.clone(),
        initial_prompt: build_initial_prompt(&config.transcription, &config.output),
        fast_encoder: config.transcription.fast_encoder,
        threads: config.transcription.threads,
    };
    let t = Instant::now();
    transcriber.warm_up();
    println!("warm_up_ms={:.0}", ms(t));

    let paste_delay = InjectionConfig::default().paste_delay_ms;
    for run in 0..runs {
        let t0 = Instant::now();
        let t = Instant::now();
        let audio = prepare_for_whisper(&raw);
        let prep = ms(t);
        let t = Instant::now();
        let text = transcriber.transcribe_with(&audio.samples, &options).expect("asr");
        let asr = ms(t);
        let t = Instant::now();
        let out = postprocess_transcription(&text, Scenario::Chat, &config.output).expect("post");
        let post = ms(t);
        // paste path: clipboard write + paste_delay + SendInput; restore is async
        std::thread::sleep(std::time::Duration::from_millis(paste_delay));
        println!(
            "run={run} prep_vad_ms={prep:.1} asr_ms={asr:.0} post_ms={post:.2} inject_wait_ms={paste_delay} total_ms={:.0} audio_s={:.2}\n  raw={text}\n  out={}",
            ms(t0),
            audio.samples.len() as f32 / 16000.0,
            out.replace('\n', "⏎")
        );
    }
}
