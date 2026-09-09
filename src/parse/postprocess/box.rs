//! 박스/하이라이트 컴포넌트 복원.
//!
//! md2hwpx는 `[[박스]]`/`===` 블록을 template.hwpx의 컴포넌트(박스: 3행 표,
//! 하이라이트: 1×1 표)로 렌더링한다. hwpx2md는 그 형상을 되돌려 md2hwpx 소스
//! 파서가 다시 먹는 명령 토큰으로 복원한다:
//!
//! - 박스 컴포넌트 표(rowspan=2 제목 셀 `< 제목 >` + colspan=3 본문 셀) →
//!   `[[박스]] 제목` + 들여쓴 `- ` 항목들(try_box/try_box_item 계약).
//!   항목 앞의 일반 불릿 글리프(`∙` 등)는 벗기고, 원문자 번호·주석 글리프는
//!   glyph.rs 분류에 따라 본문에 남긴다.
//! - 1×1 표이면서 셀 텍스트가 공백으로 시작하는 것(highlight 컴포넌트의 렌더
//!   서명: 항목 앞 빈 불릿 자리가 공백으로 남는다) → `===` 블록.
//!
//! 왕복 편차(Python 참조 대비): Python은 `[박스]`/`[box]` 접두 줄을 인용구
//! (`> `)로 표준화했지만, md2hwpx의 실제 박스 명령은 `[[박스]]`(dialect::BOX)
//! 이므로 그대로 되돌린다. 이미 올바른 `[[박스]]` 줄은 이 규칙의 정규식에
//! 걸리지 않아 그대로 유지된다.

use std::sync::LazyLock;

use regex::Regex;

use crate::shared::dialect;
use crate::shared::glyph::{classify, is_bullet_glyph};

static BOX_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*[\[［](박스|box)[\]］]\s*(.*)$").unwrap());
/// 박스 컴포넌트 표의 제목 셀: `< 제목 >`(rowspan=2로 꾸며진 셀).
static BOX_TABLE_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"rowspan="2">\s*<\s*(.+?)\s*>\s*</td>"#).unwrap());
/// 박스 컴포넌트 표의 본문 셀: colspan=3, 항목은 <br>로 이어진다.
static BOX_TABLE_BODY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"colspan="3">(.*?)</td>"#).unwrap());
/// highlight 컴포넌트 표: 1×1이고 셀 텍스트가 공백으로 시작한다.
static HIGHLIGHT_TABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<table><tr><td>(\s+.*?)</td></tr></table>$").unwrap());

/// `[박스]`/`[box]` 접두 줄을 `[[박스]]` 명령 줄로 되돌린다.
fn rewrite_box_prefix(line: &str) -> Option<String> {
    let m = BOX_PREFIX.captures(line)?;
    let body = m[2].trim();
    Some(if body.is_empty() {
        dialect::BOX.to_string()
    } else {
        format!("{} {body}", dialect::BOX)
    })
}

/// 항목 텍스트에서 일반 불릿 글리프를 벗긴다. 원문자 번호·주석 글리프는
/// md2hwpx의 resolve_item_glyph 규약대로 본문에 남긴다.
fn strip_plain_bullet(item: &str) -> String {
    match item.chars().next() {
        Some(g) if is_bullet_glyph(g) && !classify(g).is_some_and(|r| r.stays_in_body()) => {
            item[g.len_utf8()..].trim_start().to_string()
        }
        _ => item.to_string(),
    }
}

/// 박스 컴포넌트 표 한 줄을 `[[박스]]` + 항목 줄들로 복원한다.
fn recover_box_table(line: &str) -> Option<String> {
    if !line.starts_with("<table>") {
        return None;
    }
    let title = BOX_TABLE_TITLE.captures(line)?;
    let body = BOX_TABLE_BODY.captures(line)?;
    let mut out = vec![format!("{} {}", dialect::BOX, title[1].trim())];
    for item in body[1].split("<br>") {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let body_text = strip_plain_bullet(item);
        if body_text.is_empty() {
            continue;
        }
        // try_box_item 계약: 박스 명령보다 깊은 들여쓰기의 불릿 줄만 수집한다.
        out.push(format!("  - {body_text}"));
    }
    Some(out.join("\n"))
}

/// highlight 컴포넌트 표 한 줄을 `===` 블록으로 복원한다. 항목은 `- ` 줄로
/// 내보낸다 — md2hwpx의 finish_highlight가 이 접두를 벗겨 한 블록으로
/// 합친다(이미 불릿 글리프로 시작하는 항목은 이중 접두를 피한다).
fn recover_highlight_table(line: &str) -> Option<String> {
    let m = HIGHLIGHT_TABLE.captures(line)?;
    let mut out = vec![dialect::HIGHLIGHT.to_string()];
    for item in m[1].split("<br>") {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let body_text = strip_plain_bullet(item);
        if body_text.is_empty() {
            continue;
        }
        out.push(format!("- {body_text}"));
    }
    if out.len() == 1 {
        return None;
    }
    out.push(dialect::HIGHLIGHT.to_string());
    Some(out.join("\n"))
}

/// 박스/하이라이트 컴포넌트 형상과 `[박스]` 접두 줄을 md2hwpx 명령 토큰으로
/// 되돌린다. 컴포넌트가 아닌 `<table>`/일반 줄은 그대로 유지된다.
pub(crate) fn restore_boxes(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if let Some(recovered) = recover_box_table(&line)
            .or_else(|| recover_highlight_table(&line))
            .or_else(|| rewrite_box_prefix(&line))
        {
            out.extend(recovered.split('\n').map(str::to_string));
        } else {
            out.push(line);
        }
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
    fn box_prefix_becomes_box_command() {
        assert_eq!(
            restore_boxes(lines(&["[박스] 요약 내용"])),
            lines(&["[[박스]] 요약 내용"])
        );
        assert_eq!(
            restore_boxes(lines(&["[Box] summary"])),
            lines(&["[[박스]] summary"])
        );
        // 본문이 비면 명령 줄만 남는다.
        assert_eq!(restore_boxes(lines(&["[박스]"])), lines(&["[[박스]]"]));
    }

    #[test]
    fn already_correct_box_commands_pass_through() {
        // `[[박스]]`는 정규식에 걸리지 않아 이중 치환되지 않는다.
        let src = lines(&["[[박스]] 이미 올바른 제목", "일반 문단"]);
        assert_eq!(restore_boxes(src.clone()), src);
    }

    #[test]
    fn non_box_brackets_are_untouched() {
        let src = lines(&["[참고] 각주 모음", "[박스아님] x"]);
        assert_eq!(restore_boxes(src.clone()), src);
    }

    #[test]
    fn box_component_table_becomes_box_command_with_items() {
        let table = r#"<table><tr><td></td><td rowspan="2">< 기대 효과 요약 ></td><td></td></tr><tr><td></td><td></td></tr><tr><td colspan="3">∙ (경제성) 발전량 증가에 따른 전기요금 절감 효과<br>① 연간 발전량: 약 24MWh<br>☞ 주의 사항</td></tr></table>"#;
        assert_eq!(
            restore_boxes(lines(&[table])),
            lines(&[
                "[[박스]] 기대 효과 요약",
                "  - (경제성) 발전량 증가에 따른 전기요금 절감 효과",
                "  - ① 연간 발전량: 약 24MWh",
                "  - ☞ 주의 사항",
            ])
        );
    }

    #[test]
    fn highlight_component_table_becomes_highlight_block() {
        let table = "<table><tr><td> 본 문서는 Markdown → HWPX 변환 확인을 위한 예시 문서임</td></tr></table>";
        assert_eq!(
            restore_boxes(lines(&[table])),
            lines(&[
                "===",
                "- 본 문서는 Markdown → HWPX 변환 확인을 위한 예시 문서임",
                "==="
            ])
        );
    }

    #[test]
    fn plain_data_tables_are_not_components() {
        let src = lines(&[
            "<table><tr><td>구분</td><td>결과</td></tr><tr><td>빌드</td><td>성공</td></tr></table>",
        ]);
        assert_eq!(restore_boxes(src.clone()), src);
    }
}
