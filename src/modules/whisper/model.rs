use anyhow::Result;
use reqwest::blocking::get;
use std::fs::{self, File};
use std::path::Path;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const MODELS_DIR: &str = "models";

pub struct WhisperModel {
    pub context: WhisperContext,
    pub model_name: String,
}

impl WhisperModel {
    /// 載入或下載模型（支援 CUDA）
    pub fn load_or_download(model_name: &str) -> Result<Self> {
        fs::create_dir_all(MODELS_DIR)?;

        let model_path = format!("{}/ggml-{}.bin", MODELS_DIR, model_name);
        let path = Path::new(&model_path);

        if !path.exists() {
            println!("模型 {} 不存在，正在下載...", model_name);
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("模型檔名無效: {}", model_path))?;
            let url = format!("{}/{}", MODEL_BASE_URL, file_name);
            let mut response = get(&url)?.error_for_status()?;
            let temp_path = path.with_extension("bin.part");

            let mut output = File::create(&temp_path)?;
            response.copy_to(&mut output)?;
            fs::rename(&temp_path, &model_path)?;
            println!("下載完成: {}", model_path);
        } else {
            println!("找到已存在模型: {}", model_path);
        }

        // 使用 CUDA 加速（如果已編譯支援）
        let params = WhisperContextParameters {
            use_gpu: true,
            ..Default::default()
        };

        let ctx = WhisperContext::new_with_params(&model_path, params)?;

        Ok(Self {
            context: ctx,
            model_name: model_name.to_string(),
        })
    }

    /// 進行語音轉文字（支援指定語言）
    pub fn transcribe(&self, audio_samples: &[f32], language: Option<&str>) -> Result<String> {
        let mut state: WhisperState = self.context.create_state()?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // 設定語言
        if let Some(lang) = language {
            params.set_language(Some(lang));
        } else {
            params.set_language(Some("zh")); // 預設中文
        }

        // 關閉不必要的輸出
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // 執行推論
        state.full(params, audio_samples)?;

        // 收集結果
        let num_segments = state.full_n_segments();
        let mut result = String::new();

        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                result.push_str(&segment.to_str_lossy()?);
            }
        }

        Ok(result.trim().to_string())
    }

    /// 取得模型資訊
    pub fn info(&self) -> String {
        format!("{}（CUDA 已啟用）", self.model_name)
    }
}
