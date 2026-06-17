use crate::modules::config::{
    OutputConfig, OutputRulesConfig, ScenarioOutputRules, VocabularyEntry,
};
use crate::modules::scenario::Scenario;
use crate::modules::text_conversion::convert_chinese_text;

struct ProtectedTerm {
    placeholder: String,
    replacement: String,
}

pub fn postprocess_transcription(
    text: &str,
    scenario: Scenario,
    output: &OutputConfig,
) -> Result<String, String> {
    let (protected, terms) = protect_vocabulary_terms(text, &output.vocabulary.entries);
    let converted = convert_chinese_text(&protected, output.chinese_conversion)?;
    let restored = restore_vocabulary_terms(converted, &terms);
    let replaced = apply_vocabulary_replacements(restored, &output.vocabulary.entries);
    let rules = rules_for_scenario(&output.rules, scenario);

    Ok(apply_output_rules(&replaced, rules))
}

fn rules_for_scenario(rules: &OutputRulesConfig, scenario: Scenario) -> &ScenarioOutputRules {
    match scenario {
        Scenario::Chat => &rules.chat,
        Scenario::Writing => &rules.writing,
        Scenario::Code => &rules.code,
    }
}

fn protect_vocabulary_terms(
    text: &str,
    entries: &[VocabularyEntry],
) -> (String, Vec<ProtectedTerm>) {
    let mut result = text.to_string();
    let mut terms = Vec::new();

    let mut sorted: Vec<&VocabularyEntry> = entries
        .iter()
        .filter(|entry| entry.protect && !entry.is_blank())
        .collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.source.len()));

    for entry in sorted {
        let source = entry.source.trim();
        if source.is_empty() {
            continue;
        }

        let replacement = final_replacement(entry);
        let placeholder = format!("__SPEAKTYPE_TERM_{}__", terms.len());
        result = result.replace(source, &placeholder);

        if replacement != source {
            result = result.replace(&replacement, &placeholder);
        }

        terms.push(ProtectedTerm {
            placeholder,
            replacement,
        });
    }

    (result, terms)
}

fn restore_vocabulary_terms(mut text: String, terms: &[ProtectedTerm]) -> String {
    for term in terms {
        text = text.replace(&term.placeholder, &term.replacement);
    }
    text
}

fn apply_vocabulary_replacements(mut text: String, entries: &[VocabularyEntry]) -> String {
    let mut sorted: Vec<&VocabularyEntry> = entries.iter().filter(|entry| !entry.is_blank()).collect();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.source.len()));

    for entry in sorted {
        let source = entry.source.trim();
        if source.is_empty() {
            continue;
        }

        let replacement = final_replacement(entry);
        text = text.replace(source, &replacement);
    }
    text
}

fn final_replacement(entry: &VocabularyEntry) -> String {
    let replacement = entry.replacement.trim();
    if replacement.is_empty() {
        entry.source.trim().to_string()
    } else {
        replacement.to_string()
    }
}

fn apply_output_rules(text: &str, rules: &ScenarioOutputRules) -> String {
    let mut text = text.trim().to_string();

    if rules.remove_fillers {
        text = remove_fillers(&text);
    }

    if rules.preserve_code_symbols {
        text = preserve_code_symbols(&text);
    }

    if rules.auto_punctuation {
        text = ensure_terminal_punctuation(&text);
    }

    if rules.format_paragraphs {
        text = format_paragraphs(&text);
    } else if rules.auto_line_breaks {
        text = auto_line_breaks(&text);
    }

    text.trim().to_string()
}

fn remove_fillers(text: &str) -> String {
    let fillers = ["嗯", "呃", "啊", "就是", "然後", "那個", "這個", "就是說"];

    fillers
        .iter()
        .fold(text.to_string(), |acc, filler| acc.replace(filler, ""))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn preserve_code_symbols(text: &str) -> String {
    text.replace(" ，", ",")
        .replace("，", ", ")
        .replace("。", ".")
        .replace(" ：", ":")
        .replace("：", ":")
        .replace("；", ";")
        .replace("（", "(")
        .replace("）", ")")
        .replace("［", "[")
        .replace("］", "]")
        .replace("｛", "{")
        .replace("｝", "}")
        .replace("「", "\"")
        .replace("」", "\"")
        .replace("“", "\"")
        .replace("”", "\"")
        .replace("’", "'")
        .replace("‘", "'")
}

fn ensure_terminal_punctuation(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.ends_with(['。', '！', '？', '.', '!', '?', ':', '：']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}。")
    }
}

fn format_paragraphs(text: &str) -> String {
    text.split(['。', '！', '？'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| format!("{part}。"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn auto_line_breaks(text: &str) -> String {
    text.replace("。", "。\n")
        .replace("！", "！\n")
        .replace("？", "？\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::config::{
        ChineseConversionMode, OutputConfig, VocabularyConfig, VocabularyEntry,
    };

    #[test]
    fn protects_custom_terms_from_conversion() {
        let output = OutputConfig {
            chinese_conversion: ChineseConversionMode::TraditionalTaiwan,
            vocabulary: VocabularyConfig {
                entries: vec![VocabularyEntry {
                    source: "OpenAI API".to_string(),
                    replacement: "OpenAI API".to_string(),
                    protect: true,
                }],
            },
            ..Default::default()
        };

        let text = postprocess_transcription("使用 OpenAI API", Scenario::Chat, &output).unwrap();

        assert!(text.contains("OpenAI API"));
    }

    #[test]
    fn applies_code_symbol_rules() {
        let mut output = OutputConfig::default();
        output.rules.code.preserve_code_symbols = true;

        let text = postprocess_transcription("foo（bar）：baz", Scenario::Code, &output).unwrap();

        assert_eq!(text, "foo(bar):baz");
    }
}
