//! Local (offline) dictation cleanup in the spirit of Typeless:
//! filler removal, self-correction resolution, list formatting, CJK/Latin
//! spacing and punctuation normalisation. Everything here is deterministic and
//! runs in well under a millisecond, so it is always on; the optional LLM pass
//! (`ai_cleanup`) runs after it.

const CLAUSE_PUNCT: &[char] = &['，', ',', '。', '！', '？', '!', '?', '；', ';', '、', '\n'];
const SENTENCE_END: &[char] = &['。', '！', '？', '!', '?', '.'];

/// Fillers removed when they make up a whole clause or open a clause.
const LEADING_FILLERS: &[&str] = &[
    "嗯嗯", "嗯", "呃", "額", "欸", "誒", "噢", "喔", "啊", "那個", "就是說", "就是", "然後呢",
    "um", "uh", "uhm", "erm", "er", "hmm", "you know", "like",
];
/// Clauses consisting only of these are dropped.
const PURE_FILLERS: &[&str] = &[
    "嗯", "嗯嗯", "呃", "額", "欸", "誒", "啊", "那個", "就是", "就是說", "然後", "對", "um", "uh",
    "uhm", "erm", "er", "hmm", "you know", "like", "so",
];
/// "X, no wait, Y" style markers. The clause after the marker replaces the end
/// of the clause before it.
const CORRECTION_MARKERS: &[&str] = &[
    "不對不對", "不對", "不是不是", "我是說", "我的意思是", "應該說", "更正", "講錯了", "說錯了",
    "no wait", "i mean", "sorry", "actually no", "correction",
];
/// Markers that delete the previous clause outright.
const DELETE_MARKERS: &[&str] = &["刪掉上一句", "上一句不要", "scratch that", "delete that"];

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

fn is_latin_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        let rest = &text[prefix.len()..];
        // Latin fillers must be whole words.
        let latin = prefix.chars().all(|c| c.is_ascii());
        if latin && rest.chars().next().is_some_and(is_latin_word) {
            return None;
        }
        Some(rest)
    } else {
        None
    }
}

/// Split into (clause, trailing punctuation) pairs.
fn split_clauses(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut clause = String::new();
    let mut punct = String::new();
    for ch in text.chars() {
        if CLAUSE_PUNCT.contains(&ch) || (ch == '.' && !clause.ends_with(|c: char| c.is_ascii_digit())) {
            punct.push(ch);
        } else {
            if !punct.is_empty() {
                out.push((std::mem::take(&mut clause), std::mem::take(&mut punct)));
            }
            clause.push(ch);
        }
    }
    if !clause.is_empty() || !punct.is_empty() {
        out.push((clause, punct));
    }
    out
}

fn join_clauses(clauses: &[(String, String)]) -> String {
    let mut out = String::new();
    for (clause, punct) in clauses {
        let clause = clause.trim();
        if clause.is_empty() && out.is_empty() {
            continue;
        }
        if !out.is_empty() && !clause.is_empty() {
            let last = out.chars().last().unwrap_or(' ');
            let first = clause.chars().next().unwrap_or(' ');
            if (last.is_ascii_punctuation() || is_latin_word(last)) && is_latin_word(first) {
                out.push(' ');
            }
        }
        out.push_str(clause);
        out.push_str(punct.trim_matches(' '));
    }
    out
}

pub fn remove_fillers(text: &str) -> String {
    let clauses = split_clauses(text)
        .into_iter()
        .filter_map(|(clause, punct)| {
            let mut body = clause.trim().to_string();
            loop {
                let mut changed = false;
                for filler in LEADING_FILLERS {
                    if let Some(rest) = strip_prefix_ci(&body, filler) {
                        let rest = rest.trim_start_matches([' ', ',', '，']).to_string();
                        if !rest.is_empty() {
                            body = rest;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            let lowered = body.trim().to_lowercase();
            if PURE_FILLERS.iter().any(|f| *f == lowered) {
                // keep sentence-ending punctuation for the previous clause
                return if punct.contains(SENTENCE_END) {
                    Some((String::new(), punct))
                } else {
                    None
                };
            }
            // collapse stutters like 嗯嗯 / 我我我
            Some((body, punct))
        })
        .collect::<Vec<_>>();
    let mut joined = join_clauses(&clauses);
    for f in ["嗯", "呃"] {
        joined = joined.replace(f, "");
    }
    joined
}

fn common_suffix_chars(a: &[char], b: &[char]) -> usize {
    a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count()
}

fn common_prefix_chars(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Merge `previous` with its correction.
fn apply_correction(previous: &str, correction: &str) -> String {
    let correction = correction.trim();
    let correction = correction
        .strip_prefix("是")
        .or_else(|| strip_prefix_ci(correction, "it's "))
        .unwrap_or(correction)
        .trim();
    if correction.is_empty() {
        return previous.to_string();
    }
    let p: Vec<char> = previous.trim().chars().collect();
    let c: Vec<char> = correction.chars().collect();
    let suffix = common_suffix_chars(&p, &c);
    if suffix > 0 && suffix < c.len() {
        let c_core = &c[..c.len() - suffix];
        let p_core = &p[..p.len() - suffix];
        // "下午3點開會，不對，14點開會": replace the whole number, not N chars.
        let keep = if c_core.first().is_some_and(|ch| ch.is_ascii_digit()) {
            p_core
                .iter()
                .rposition(|ch| ch.is_ascii_digit())
                .map(|end| {
                    let mut start = end;
                    while start > 0 && p_core[start - 1].is_ascii_digit() {
                        start -= 1;
                    }
                    start
                })
                .unwrap_or_else(|| p_core.len().saturating_sub(c_core.len()))
        } else {
            p_core.len().saturating_sub(c_core.len())
        };
        return p_core[..keep].iter().chain(c.iter()).collect();
    }
    let prefix = common_prefix_chars(&p, &c);
    if prefix > 0 {
        return correction.to_string();
    }
    if c.len() < p.len() && !p.is_empty() {
        // "下午三點，不對，四點" -> replace the tail of the same length
        let keep = p.len() - c.len();
        return p[..keep].iter().chain(c.iter()).collect();
    }
    correction.to_string()
}

pub fn resolve_self_corrections(text: &str) -> String {
    let clauses = split_clauses(text);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut pending_correction = false;
    for (clause, punct) in clauses {
        let trimmed = clause.trim();
        let lowered = trimmed.to_lowercase();
        if DELETE_MARKERS.iter().any(|m| lowered == *m) {
            out.pop();
            continue;
        }
        let mut body = trimmed.to_string();
        let mut marker_found = false;
        for marker in CORRECTION_MARKERS {
            if lowered == *marker {
                pending_correction = true;
                marker_found = true;
                break;
            }
            if let Some(rest) = strip_prefix_ci(trimmed, marker) {
                body = rest.trim_start_matches([' ', ',', '，']).to_string();
                pending_correction = true;
                break;
            }
        }
        if marker_found {
            continue;
        }
        if pending_correction {
            pending_correction = false;
            if let Some(last) = out.last_mut() {
                last.0 = apply_correction(&last.0, &body);
                last.1 = punct;
                continue;
            }
        }
        out.push((body, punct));
    }
    join_clauses(&out)
}

const ORDINALS_ZH: &[&[&str]] = &[
    &["第一", "第二", "第三", "第四", "第五", "第六", "第七", "第八", "第九", "第十"],
    &["首先", "其次", "再來", "最後"],
];
const ORDINALS_EN: &[&str] = &["first", "second", "third", "fourth", "fifth", "sixth"];

/// Turn spoken enumerations into a numbered list. Needs at least two markers.
pub fn format_lists(text: &str) -> String {
    // Locate ordinal markers at clause starts.
    let clauses = split_clauses(text);
    let mut items: Vec<(usize, String)> = Vec::new(); // (clause index, item body)
    let mut expected = 0usize;
    let mut family: Option<usize> = None;
    for (idx, (clause, _)) in clauses.iter().enumerate() {
        let body = clause.trim();
        let mut hit = None;
        for (fi, fam) in ORDINALS_ZH.iter().enumerate() {
            if family.is_some_and(|f| f != fi) {
                continue;
            }
            let want = fam.get(expected.min(fam.len() - 1)).copied();
            let candidates: Vec<&str> = if fi == 1 && expected >= 1 {
                fam[1..].to_vec()
            } else {
                want.into_iter().collect()
            };
            for marker in candidates {
                let stripped = body
                    .strip_prefix("然後")
                    .unwrap_or(body)
                    .strip_prefix(marker)
                    .map(|r| r.trim_start_matches(['，', ',', '、', '個', '點', ' ', '是']).trim());
                if let Some(rest) = stripped {
                    hit = Some((fi, rest.to_string()));
                    break;
                }
            }
            if hit.is_some() {
                break;
            }
        }
        if hit.is_none() && family.is_none_or(|f| f == 9) {
            if let Some(marker) = ORDINALS_EN.get(expected) {
                if let Some(rest) = strip_prefix_ci(body, marker) {
                    hit = Some((9, rest.trim_start_matches([',', ' ']).to_string()));
                }
            }
        }
        if let Some((fi, rest)) = hit {
            if rest.is_empty() {
                // marker alone in its clause: the item is the next clause
                items.push((idx, String::new()));
            } else {
                items.push((idx, rest));
            }
            family = Some(fi);
            expected += 1;
        }
    }
    if items.len() < 2 {
        return text.to_string();
    }

    let first_idx = items[0].0;
    let mut out = String::new();
    let intro = join_clauses(&clauses[..first_idx]);
    let intro = intro.trim_end_matches(['，', ',', '；', ';']).trim().to_string();
    let intro = intro.trim_end_matches("然後").trim_end_matches(['。', '：', ':']).to_string();
    if !intro.is_empty() {
        out.push_str(&intro);
        out.push_str(if intro.chars().last().is_some_and(is_cjk) { "：" } else { ":" });
        out.push('\n');
    }
    let mut tail_start = clauses.len();
    for (n, (idx, body)) in items.iter().enumerate() {
        let end = items.get(n + 1).map(|(i, _)| *i).unwrap_or_else(|| {
            // the last item runs to the end of its sentence
            let mut j = *idx;
            while j < clauses.len() && !clauses[j].1.contains(SENTENCE_END) {
                j += 1;
            }
            (j + 1).min(clauses.len())
        });
        if n + 1 == items.len() {
            tail_start = end;
        }
        let mut parts: Vec<String> = Vec::new();
        if !body.is_empty() {
            parts.push(body.clone());
        }
        for (clause, _) in clauses.iter().take(end).skip(idx + 1) {
            if !clause.trim().is_empty() {
                parts.push(clause.trim().to_string());
            }
        }
        let sep = if parts.iter().any(|p| p.chars().any(is_cjk)) { "，" } else { ", " };
        out.push_str(&format!("{}. {}\n", n + 1, parts.join(sep)));
    }
    let tail = join_clauses(&clauses[tail_start.min(clauses.len())..]);
    if !tail.trim().is_empty() {
        out.push_str(tail.trim());
    }
    out.trim_end().to_string()
}

/// Insert a space between CJK and Latin/digits (Taiwanese typesetting convention).
pub fn cjk_spacing(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut prev: Option<char> = None;
    for ch in text.chars() {
        if let Some(p) = prev {
            let need = (is_cjk(p) && (ch.is_ascii_alphabetic() || ch.is_ascii_digit() && false))
                || ((p.is_ascii_alphabetic()) && is_cjk(ch));
            if need {
                out.push(' ');
            }
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

/// Fullwidth punctuation inside Chinese text; collapse whitespace between CJK chars.
pub fn normalize_cjk_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let prev_sig = |i: usize| chars[..i].iter().rev().find(|c| !c.is_whitespace()).copied();
    let next_sig = |i: usize| chars[i + 1..].iter().find(|c| !c.is_whitespace()).copied();
    for (i, &ch) in chars.iter().enumerate() {
        let before = prev_sig(i);
        let after = next_sig(i);
        let cjk_context = before.is_some_and(is_cjk) && (after.is_none() || after.is_some_and(|c| is_cjk(c) || c.is_ascii_alphanumeric()));
        let mapped = match ch {
            ',' if cjk_context => '，',
            '?' if cjk_context => '？',
            '!' if cjk_context => '！',
            ':' if cjk_context => '：',
            ';' if cjk_context => '；',
            '.' if cjk_context && after.is_none_or(|c| !c.is_ascii_digit()) => '。',
            ' ' if before.is_some_and(is_cjk) && after.is_some_and(is_cjk) => continue,
            ' ' if before.is_some_and(|c| "，。！？：；、".contains(c)) => continue,
            other => other,
        };
        out.push(mapped);
    }
    out
}

/// Sentence-final punctuation matching the script of the last character.
pub fn ensure_terminal_punctuation(text: &str) -> String {
    let trimmed = text.trim_end();
    let Some(last) = trimmed.chars().last() else {
        return String::new();
    };
    if "。！？.!?:：…)）」\"".contains(last) || trimmed.lines().last().is_some_and(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()) && l.contains(". ")) {
        return trimmed.to_string();
    }
    if is_cjk(last) {
        format!("{trimmed}。")
    } else {
        format!("{trimmed}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_leading_and_pure_fillers() {
        assert_eq!(remove_fillers("嗯，我們明天開會"), "我們明天開會");
        assert_eq!(remove_fillers("um, I think so"), "I think so");
        // must not remove meaningful words
        assert_eq!(remove_fillers("這個方案很好"), "這個方案很好");
        assert_eq!(remove_fillers("likely yes"), "likely yes");
    }

    #[test]
    fn resolves_self_correction_by_shared_suffix() {
        assert_eq!(
            resolve_self_corrections("我們明天下午三點開會，不對，是四點開會。"),
            "我們明天下午四點開會。"
        );
        assert_eq!(resolve_self_corrections("下午三點，不對，四點"), "下午四點");
        assert_eq!(resolve_self_corrections("下午3點開會，不對，14點開會"), "下午14點開會");
        assert_eq!(
            resolve_self_corrections("send it to John, I mean Mary"),
            "send it to Mary"
        );
        assert_eq!(resolve_self_corrections("第一句。第二句，scratch that，第三句"), "第一句。第三句");
    }

    #[test]
    fn formats_spoken_lists() {
        let out = format_lists("明天要做兩件事。第一，準備報告；第二，確認預算。其他再說。");
        assert_eq!(out, "明天要做兩件事：\n1. 準備報告\n2. 確認預算\n其他再說。");
        let out = format_lists("first, buy milk. second, call mom.");
        assert_eq!(out, "1. buy milk\n2. call mom");
        assert_eq!(format_lists("第一次來台北"), "第一次來台北");
    }

    #[test]
    fn spacing_and_punctuation() {
        assert_eq!(cjk_spacing("用GitHub跟Slack同步"), "用 GitHub 跟 Slack 同步");
        assert_eq!(normalize_cjk_punctuation("嗯,我們明天 下午開會."), "嗯，我們明天下午開會。");
        assert_eq!(ensure_terminal_punctuation("hello world"), "hello world.");
        assert_eq!(ensure_terminal_punctuation("你好"), "你好。");
    }
}
