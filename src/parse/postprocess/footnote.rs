//! 각주 asterisk 들여쓰기 (Python 참조의 직접 포트).
//!
//! 앞 줄에 단어에 붙은 asterisk(`제안*`)가 있고 바로 다음 비공백 줄이
//! `* `로 시작하면, 이 asterisk는 불릿이 아니라 앞 단어에 대한 각주
//! 설명이므로 6칸 들여쓰기로 내려 목록 최상위에서 제외한다.

use std::sync::LazyLock;

use regex::Regex;

/// 각주 참조: 단어에 붙은 asterisk (좌우가 비어 있지 않음) — `제안*`
static FOOTNOTE_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\S\*").unwrap());
static ASTERISK_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\*\s+(?P<body>.+)$").unwrap());

pub(crate) fn indent_footnote_asterisks(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let matched = ASTERISK_LINE.captures(&line);
        let prev_nonblank = out
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(String::as_str)
            .unwrap_or("");
        if let (Some(m), true) = (matched, FOOTNOTE_REF.is_match(prev_nonblank)) {
            out.push(format!("      * {}", &m["body"]));
            continue;
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn asterisk_after_word_reference_is_indented() {
        let src = lines(&["태양광 확대를 제안*하며", "* 검토 필요"]);
        assert_eq!(
            indent_footnote_asterisks(src),
            lines(&["태양광 확대를 제안*하며", "      * 검토 필요"])
        );
    }

    #[test]
    fn standalone_star_bullets_are_untouched() {
        let src = lines(&["일반 문단", "* 별표 항목"]);
        assert_eq!(indent_footnote_asterisks(src.clone()), src);
    }
}
