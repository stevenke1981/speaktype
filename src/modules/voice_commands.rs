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

pub fn parse_voice_command(text: &str) -> Option<VoiceCommand> {
    let trimmed = text.trim().to_string();
    let normalized = trimmed
        .chars()
        .filter(|c| !matches!(c, '。' | '，' | '！' | '？' | '.' | ',' | '!' | '?' | ' '))
        .collect::<String>();

    match normalized.as_str() {
        "刪掉" | "刪除" | "擦掉" | "去掉" => Some(VoiceCommand::Delete),
        "換行" | "新行" | "下一行" => Some(VoiceCommand::Newline),
        "句號" => Some(VoiceCommand::Punctuation('。')),
        "逗號" | "逗点" => Some(VoiceCommand::Punctuation('，')),
        "問號" | "问号" => Some(VoiceCommand::Punctuation('？')),
        "驚嘆號" | "感叹号" => Some(VoiceCommand::Punctuation('！')),
        "全選" | "全选" => Some(VoiceCommand::SelectAll),
        "復原" | "还原" | "取消" | "撤销" => Some(VoiceCommand::Undo),
        "下一段" | "新段落" => Some(VoiceCommand::NextParagraph),
        _ => None,
    }
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
    }

    #[test]
    fn parses_select_all() {
        assert_eq!(
            parse_voice_command("全選"),
            Some(VoiceCommand::SelectAll)
        );
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
        assert_eq!(mock.keys, vec!["ControlDown", "a", "ControlUp"]);
    }

    struct MockKeyboard {
        keys: Vec<&'static str>,
    }

    impl MockKeyboard {
        fn new() -> Self {
            Self { keys: Vec::new() }
        }
    }

    impl KeyboardControllable for MockKeyboard {
        fn key_down(&mut self, key: Key) {
            match key {
                Key::Control => self.keys.push("ControlDown"),
                _ => self.keys.push("other_down"),
            }
        }

        fn key_up(&mut self, key: Key) {
            match key {
                Key::Control => self.keys.push("ControlUp"),
                _ => self.keys.push("other_up"),
            }
        }

        fn key_click(&mut self, key: Key) {
            match key {
                Key::Backspace => self.keys.push("Backspace"),
                Key::Return => self.keys.push("Return"),
                Key::Layout(ch) => self.keys.push(Box::leak(ch.to_string().into_boxed_str())),
                _ => self.keys.push("other_click"),
            }
        }

        fn key_sequence(&mut self, sequence: &str) {
            for ch in sequence.chars() {
                self.keys.push(Box::leak(ch.to_string().into_boxed_str()));
            }
        }
    }
}
