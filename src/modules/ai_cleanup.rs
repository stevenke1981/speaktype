//! Optional LLM rewrite step (Typeless-style "AI auto-edit").
//!
//! Works with any OpenAI-compatible `chat/completions` endpoint: a local
//! Ollama / LM Studio / llama.cpp server (default, nothing leaves the PC) or a
//! cloud API with a key. Disabled by default; on any error or timeout the
//! locally cleaned text is used, so dictation never blocks on the network.
use crate::modules::config::AiCleanupConfig;
use std::time::Duration;

pub struct AiContext<'a> {
    pub scenario: &'a str,
    pub app: Option<&'a str>,
    pub tone: &'a str,
    pub dictionary: &'a [String],
    pub traditional_chinese: bool,
}

pub fn system_prompt(config: &AiCleanupConfig, ctx: &AiContext) -> String {
    let mut prompt = String::from(
        "You clean up dictated text. Rules:\n\
         - Output ONLY the cleaned text, no quotes, no explanations.\n\
         - Keep the speaker's meaning, language mix and wording; do not answer questions or add content.\n\
         - Remove filler words (嗯, 呃, 那個, um, uh, like) and false starts.\n\
         - When the speaker corrects themselves (不對, 我是說, I mean, no wait), keep only the corrected version.\n\
         - Add proper punctuation. Format spoken enumerations as a numbered list.\n\
         - Keep English words, product names and code identifiers exactly as spoken.\n",
    );
    if ctx.traditional_chinese {
        prompt.push_str("- Write Chinese in Traditional Chinese characters with Taiwan wording (繁體中文、台灣用語), fullwidth punctuation.\n");
    }
    match ctx.scenario {
        "聊天" => prompt.push_str("- Context: chat message. Casual, concise, no greeting or sign-off added.\n"),
        "寫作" => prompt.push_str("- Context: document or email. Clear, well-structured, paragraphs where natural.\n"),
        "程式碼" => prompt.push_str("- Context: code editor or terminal. Keep symbols, no sentence-final period, no list formatting.\n"),
        _ => {}
    }
    if let Some(app) = ctx.app {
        prompt.push_str(&format!("- Target application: {app}.\n"));
    }
    if !ctx.tone.trim().is_empty() {
        prompt.push_str(&format!("- Tone: {}.\n", ctx.tone.trim()));
    }
    if !ctx.dictionary.is_empty() {
        prompt.push_str(&format!(
            "- Preferred spellings (personal dictionary): {}.\n",
            ctx.dictionary.join(", ")
        ));
    }
    if !config.custom_instructions.trim().is_empty() {
        prompt.push_str(&format!("- {}\n", config.custom_instructions.trim()));
    }
    prompt
}

pub fn build_request_body(config: &AiCleanupConfig, ctx: &AiContext, text: &str) -> serde_json::Value {
    serde_json::json!({
        "model": config.model,
        "temperature": 0.1,
        "stream": false,
        "messages": [
            {"role": "system", "content": system_prompt(config, ctx)},
            {"role": "user", "content": text},
        ]
    })
}

pub fn parse_response(body: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("AI 回應不是 JSON: {e}"))?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .or_else(|| value.pointer("/message/content").and_then(|v| v.as_str()))
        .ok_or_else(|| "AI 回應缺少 content".to_string())?;
    // strip <think> blocks from reasoning models and wrapping quotes
    let mut text = content.to_string();
    while let (Some(s), Some(e)) = (text.find("<think>"), text.find("</think>")) {
        if e < s {
            break;
        }
        text.replace_range(s..e + "</think>".len(), "");
    }
    let text = text.trim().trim_matches(['"', '「', '」']).trim().to_string();
    if text.is_empty() {
        return Err("AI 回應為空".to_string());
    }
    Ok(text)
}

pub fn api_key(config: &AiCleanupConfig) -> Option<String> {
    let key = config.api_key.trim();
    if !key.is_empty() {
        return Some(key.to_string());
    }
    std::env::var("SPEAKTYPE_AI_API_KEY").ok().filter(|k| !k.trim().is_empty())
}

/// Rewrite `text`; returns Err on failure so the caller can keep the local result.
pub fn rewrite(config: &AiCleanupConfig, ctx: &AiContext, text: &str) -> Result<String, String> {
    if !config.enabled || text.chars().count() < config.min_chars {
        return Ok(text.to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms.max(500)))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .post(config.endpoint.trim())
        .header("Content-Type", "application/json")
        .body(build_request_body(config, ctx, text).to_string());
    if let Some(key) = api_key(config) {
        request = request.bearer_auth(key);
    }
    let response = request.send().map_err(|e| format!("AI 潤稿連線失敗: {e}"))?;
    let status = response.status();
    let body = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("AI 潤稿失敗 HTTP {status}: {}", body.chars().take(200).collect::<String>()));
    }
    let out = parse_response(&body)?;
    // Guard against the model answering instead of editing.
    let ratio = out.chars().count() as f32 / text.chars().count().max(1) as f32;
    if ratio > 2.5 {
        return Err("AI 回應長度異常，已改用本地結果".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> AiContext<'static> {
        AiContext { scenario: "聊天", app: Some("LINE.exe"), tone: "", dictionary: &[], traditional_chinese: true }
    }

    #[test]
    fn prompt_mentions_traditional_and_context() {
        let p = system_prompt(&AiCleanupConfig::default(), &ctx());
        assert!(p.contains("繁體中文"));
        assert!(p.contains("LINE.exe"));
        assert!(p.contains("chat message"));
    }

    #[test]
    fn parses_openai_and_ollama_shapes() {
        let openai = r#"{"choices":[{"message":{"role":"assistant","content":"<think>x</think> 你好。"}}]}"#;
        assert_eq!(parse_response(openai).unwrap(), "你好。");
        let ollama = r#"{"message":{"content":"\"hi\""}}"#;
        assert_eq!(parse_response(ollama).unwrap(), "hi");
        assert!(parse_response("{}").is_err());
    }

    #[test]
    fn disabled_is_passthrough() {
        let cfg = AiCleanupConfig::default();
        assert_eq!(rewrite(&cfg, &ctx(), "abc").unwrap(), "abc");
    }

    #[test]
    fn local_mock_server_roundtrip() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf);
            let body = r#"{"choices":[{"message":{"content":"我們四點開會。"}}]}"#;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
        });
        let cfg = AiCleanupConfig {
            enabled: true,
            endpoint: format!("http://{addr}/v1/chat/completions"),
            min_chars: 1,
            ..Default::default()
        };
        assert_eq!(rewrite(&cfg, &ctx(), "嗯我們三點不對四點開會").unwrap(), "我們四點開會。");
    }
}
