use anyhow::Result;
use speaktype::modules::transcription::Transcriber;
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("=== SpeakType Transcriber Test ===");

    // 預設模型路徑（請根據你的實際位置修改）
    let model_path = PathBuf::from("models/ggml-large-v3-turbo.bin");

    if !model_path.exists() {
        println!("找不到模型檔案: {:?}", model_path);
        println!("請先下載模型到 models/ 資料夾");
        return Ok(());
    }

    println!("載入模型: {:?}", model_path);
    let transcriber = Transcriber::new(&model_path, true)?;

    // TODO: 這裡之後會接真實的錄音 buffer
    // 目前先用空的 buffer 測試是否能正常初始化
    let dummy_audio: Vec<f32> = vec![0.0; 16000 * 3]; // 3 秒靜音

    println!("開始轉錄測試（3秒靜音）...");
    let result = transcriber.transcribe(&dummy_audio)?;

    println!("轉錄結果: \"{}\"", result);
    println!("測試完成！");

    Ok(())
}
