// scenario/mod.rs - 情境模板與語言切換模組
// 職責：提供聊天、寫作、程式碼等情境模板，自動語言偵測

/// 情境類型
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scenario {
    Chat,    // 聊天
    Writing, // 寫作
    Code,    // 程式碼
}

impl Scenario {
    pub fn name(&self) -> &'static str {
        match self {
            Scenario::Chat => "聊天",
            Scenario::Writing => "寫作",
            Scenario::Code => "程式碼",
        }
    }

    pub fn all() -> Vec<Scenario> {
        vec![Scenario::Chat, Scenario::Writing, Scenario::Code]
    }

    pub fn from_name(name: &str) -> Option<Scenario> {
        match name {
            "聊天" => Some(Scenario::Chat),
            "寫作" => Some(Scenario::Writing),
            "程式碼" => Some(Scenario::Code),
            _ => None,
        }
    }

    pub fn postprocess(&self, text: &str) -> String {
        match self {
            Scenario::Chat => postprocess_chat(text),
            Scenario::Writing => postprocess_writing(text),
            Scenario::Code => postprocess_code(text),
        }
    }
}

fn postprocess_chat(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.ends_with(['。', '！', '？', '.', '!', '?', ':', '：']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}。")
    }
}

fn postprocess_writing(text: &str) -> String {
    text.split(['。', '！', '？'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| format!("{part}。"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn postprocess_code(text: &str) -> String {
    text.replace(" ，", ",")
        .replace("。", ".")
        .replace(" ：", ":")
        .replace("；", ";")
        .replace("（", "(")
        .replace("）", ")")
        .replace("［", "[")
        .replace("］", "]")
        .replace("｛", "{")
        .replace("｝", "}")
        .trim()
        .to_string()
}

pub struct ScenarioManager {
    current: Scenario,
}

impl ScenarioManager {
    pub fn new() -> Self {
        Self {
            current: Scenario::Chat,
        }
    }

    pub fn with_current(current: Scenario) -> Self {
        Self { current }
    }

    pub fn current(&self) -> Scenario {
        self.current
    }

    pub fn select(&mut self, scenario: Scenario) {
        self.current = scenario;
    }
}

impl Default for ScenarioManager {
    fn default() -> Self {
        Self::new()
    }
}
