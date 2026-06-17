// voice_commands.rs - 語音指令解析器
// 當使用者說出特定指令時，執行對應動作而非輸出文字

use enigo::{Key, KeyboardControllable};

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceCommand {
    Delete,
    Newline,
    Punctuation(char),
    SelectAll,
    Undo,
    NextParagraph,
}

/// 清除語音轉錄中常見的干擾字元（標點、空格），保留中文核心內容
fn clean_text(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '。' | '，' | '！' | '？' | '、' | '；' | '：'
                    | '.' | ',' | '!' | '?' | ';' | ':'
                    | ' ' | '\u{3000}' // 全形空格
                    | '「' | '」' | '『' | '』' | '（' | '）'
                    | '\u{201c}' | '\u{201d}' // 左右雙引號
            )
        })
        .collect()
}

/// Whisper 常在指令前插入的無意義前綴詞
const PREFIX_NOISE: &[&str] = &[
    "插入", "加入",
    "辨識", "識別", "辨別",
    "幫我", "請幫我", "請", "幫",
    "給我",
];

/// 剝除前綴干擾詞，直到無法再剝
fn strip_prefixes(s: &str) -> String {
    let mut result = s.to_string();
    loop {
        let before = result.clone();
        for prefix in PREFIX_NOISE {
            if result.starts_with(prefix) {
                result = result[prefix.len()..].to_string();
                break;
            }
        }
        if result == before {
            break;
        }
    }
    result
}

/// 比對指令（已清除干擾後）
fn match_command(s: &str) -> Option<VoiceCommand> {
    // ── 刪除 ──
    // 支援繁體、簡體、常見誤聽（翻掉、擦掉了）
    if matches!(
        s,
        "刪掉" | "刪除" | "擦掉" | "去掉"
            | "刪除掉" | "刪去了"
            | "删掉" | "删除了" | "删除"
            | "翻掉" | "翻掉了"
            | "擦掉了"
    ) {
        return Some(VoiceCommand::Delete);
    }

    // ── 換行 ──
    if matches!(s, "換行" | "新行" | "下一行" | "換行符") {
        return Some(VoiceCommand::Newline);
    }

    // ── 句號 ──
    if matches!(s, "句號" | "句好" | "巨號" | "具號" | "句号") {
        return Some(VoiceCommand::Punctuation('。'));
    }

    // ── 逗號 ──
    // Whisper 常見同音變體：豆號、鬥號、斗號、豆浩
    if matches!(s, "逗號" | "逗点" | "豆號" | "鬥號" | "斗號" | "豆浩" | "逗号") {
        return Some(VoiceCommand::Punctuation('，'));
    }

    // ── 問號 ──
    if matches!(s, "問號" | "问号") {
        return Some(VoiceCommand::Punctuation('？'));
    }

    // ── 驚嘆號 ──
    if matches!(s, "驚嘆號" | "感叹号" | "驚歎號" | "驚嘆好" | "惊叹号") {
        return Some(VoiceCommand::Punctuation('！'));
    }

    // ── 全選 ──
    if matches!(s, "全選" | "全选" | "圈選") {
        return Some(VoiceCommand::SelectAll);
    }

    // ── 復原 ──
    if matches!(s, "復原" | "还原" | "取消" | "撤销" | "復元" | "回復" | "复原") {
        return Some(VoiceCommand::Undo);
    }

    // ── 下一段 ──
    if matches!(s, "下一段" | "新段落" | "下個段落") {
        return Some(VoiceCommand::NextParagraph);
    }

    None
}

/// 比對標準指令及其常見 ASR 同音／近似變體
pub fn parse_voice_command(text: &str) -> Option<VoiceCommand> {
    let s = clean_text(text);
    if s.is_empty() {
        return None;
    }

    // 先試直接比對
    if let Some(cmd) = match_command(&s) {
        return Some(cmd);
    }

    // 再試剝除前綴後比對
    let stripped = strip_prefixes(&s);
    if stripped != s {
        if let Some(cmd) = match_command(&stripped) {
            return Some(cmd);
        }
    }

    None
}

pub fn execute_voice_command(command: &VoiceCommand, input: &mut impl KeyboardControllable) {
    match command {
        VoiceCommand::Delete => {
            input.key_click(Key::Backspace);
        }
        VoiceCommand::Newline => {
            input.key_click(Key::Return);
        }
        VoiceCommand::Punctuation(ch) => {
            input.key_sequence(&ch.to_string());
        }
        VoiceCommand::SelectAll => {
            input.key_down(Key::Control);
            input.key_click(Key::Layout('a'));
            input.key_up(Key::Control);
        }
        VoiceCommand::Undo => {
            input.key_down(Key::Control);
            input.key_click(Key::Layout('z'));
            input.key_up(Key::Control);
        }
        VoiceCommand::NextParagraph => {
            input.key_click(Key::Return);
            input.key_click(Key::Return);
        }
    }
}

pub fn voice_command_label(command: &VoiceCommand) -> &'static str {
    match command {
        VoiceCommand::Delete => "刪掉／刪除",
        VoiceCommand::Newline => "換行",
        VoiceCommand::Punctuation('。') => "句號",
        VoiceCommand::Punctuation('，') => "逗號",
        VoiceCommand::Punctuation('？') => "問號",
        VoiceCommand::Punctuation('！') => "驚嘆號",
        VoiceCommand::Punctuation(_) => "標點",
        VoiceCommand::SelectAll => "全選",
        VoiceCommand::Undo => "復原",
        VoiceCommand::NextParagraph => "下一段",
    }
}

pub fn voice_command_description(command: &VoiceCommand) -> &'static str {
    match command {
        VoiceCommand::Delete => "刪除上一個字",
        VoiceCommand::Newline => "插入換行",
        VoiceCommand::Punctuation(_) => "插入標點符號",
        VoiceCommand::SelectAll => "全選文字 (Ctrl+A)",
        VoiceCommand::Undo => "復原上一步 (Ctrl+Z)",
        VoiceCommand::NextParagraph => "插入段落間距",
    }
}

pub fn available_commands() -> Vec<VoiceCommand> {
    vec![
        VoiceCommand::Delete,
        VoiceCommand::Newline,
        VoiceCommand::Punctuation('。'),
        VoiceCommand::Punctuation('，'),
        VoiceCommand::Punctuation('？'),
        VoiceCommand::Punctuation('！'),
        VoiceCommand::SelectAll,
        VoiceCommand::Undo,
        VoiceCommand::NextParagraph,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delete() {
        assert_eq!(parse_voice_command("刪掉"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("刪除"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("擦掉"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("刪除掉"), Some(VoiceCommand::Delete));
        // 簡體與常見誤聽
        assert_eq!(parse_voice_command("删掉"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("翻掉"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("翻掉了"), Some(VoiceCommand::Delete));
    }

    #[test]
    fn parses_newline() {
        assert_eq!(parse_voice_command("換行"), Some(VoiceCommand::Newline));
        assert_eq!(parse_voice_command("下一行"), Some(VoiceCommand::Newline));
    }

    #[test]
    fn parses_punctuation() {
        assert_eq!(
            parse_voice_command("句號"),
            Some(VoiceCommand::Punctuation('。'))
        );
        assert_eq!(
            parse_voice_command("逗號"),
            Some(VoiceCommand::Punctuation('，'))
        );
        // ASR 同音變體
        assert_eq!(
            parse_voice_command("豆號"),
            Some(VoiceCommand::Punctuation('，'))
        );
        assert_eq!(
            parse_voice_command("鬥號"),
            Some(VoiceCommand::Punctuation('，'))
        );
        assert_eq!(
            parse_voice_command("句好"),
            Some(VoiceCommand::Punctuation('。'))
        );
        assert_eq!(
            parse_voice_command("豆浩"),
            Some(VoiceCommand::Punctuation('，'))
        );
    }

    #[test]
    fn parses_select_all() {
        assert_eq!(parse_voice_command("全選"), Some(VoiceCommand::SelectAll));
    }

    #[test]
    fn parses_undo() {
        assert_eq!(parse_voice_command("復原"), Some(VoiceCommand::Undo));
        assert_eq!(parse_voice_command("取消"), Some(VoiceCommand::Undo));
    }

    #[test]
    fn parses_next_paragraph() {
        assert_eq!(
            parse_voice_command("下一段"),
            Some(VoiceCommand::NextParagraph)
        );
    }

    #[test]
    fn ignores_postprocess_noise() {
        assert_eq!(parse_voice_command("刪掉。"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("刪除！"), Some(VoiceCommand::Delete));
        assert_eq!(parse_voice_command("刪除掉。"), Some(VoiceCommand::Delete));
        // 多餘標點應被 clean_text 濾除
        assert_eq!(
            parse_voice_command("，豆號。"),
            Some(VoiceCommand::Punctuation('，'))
        );
    }

    #[test]
    fn strips_prefix_noise() {
        // 前綴「插入」「辨識」應被剝除
        assert_eq!(
            parse_voice_command("插入豆浩"),
            Some(VoiceCommand::Punctuation('，'))
        );
        assert_eq!(
            parse_voice_command("辨識插入鬥號"),
            Some(VoiceCommand::Punctuation('，'))
        );
        assert_eq!(
            parse_voice_command("幫我刪掉"),
            Some(VoiceCommand::Delete)
        );
    }

    #[test]
    fn returns_none_for_normal_text() {
        assert_eq!(parse_voice_command("今天天氣不錯"), None);
        assert_eq!(parse_voice_command(""), None);
        assert_eq!(parse_voice_command("刪掉這行字"), None);
    }

    #[test]
    fn executes_delete() {
        let mut mock = MockKeyboard::new();
        execute_voice_command(&VoiceCommand::Delete, &mut mock);
        assert_eq!(mock.keys, vec!["Backspace"]);
    }

    #[test]
    fn executes_select_all() {
        let mut mock = MockKeyboard::new();
        execute_voice_command(&VoiceCommand::SelectAll, &mut mock);
        assert_eq!(
            mock.keys,
            vec!["ControlDown", "a", "ControlUp"] as Vec<String>
        );
    }

    struct MockKeyboard {
        keys: Vec<String>,
    }

    impl MockKeyboard {
        fn new() -> Self {
            Self { keys: Vec::new() }
        }
    }

    impl KeyboardControllable for MockKeyboard {
        fn key_down(&mut self, key: Key) {
            match key {
                Key::Control => self.keys.push("ControlDown".into()),
                _ => self.keys.push("other_down".into()),
            }
        }

        fn key_up(&mut self, key: Key) {
            match key {
                Key::Control => self.keys.push("ControlUp".into()),
                _ => self.keys.push("other_up".into()),
            }
        }

        fn key_click(&mut self, key: Key) {
            match key {
                Key::Backspace => self.keys.push("Backspace".into()),
                Key::Return => self.keys.push("Return".into()),
                Key::Layout(ch) => self.keys.push(ch.to_string()),
                _ => self.keys.push("other_click".into()),
            }
        }

        fn key_sequence(&mut self, sequence: &str) {
            for ch in sequence.chars() {
                self.keys.push(ch.to_string());
            }
        }
    }
}
