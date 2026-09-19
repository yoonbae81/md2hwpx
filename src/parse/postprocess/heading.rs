//! 표→헤딩 승격 규칙과, 표 배출(parse.rs)과 승격이 같이 쓰는 판정 가드.
//!
//! "같은 판정이 배출과 승격에서 갈라지지 않게" 하는 계약을 구조적으로
//! 지키기 위해 `is_subtitle_text`/`title_bar_heading_from_cells`는 이
//! 모듈에만 정의하고 parse.rs가 import한다.
//!
//! 왕복 편차(Python 참조 markdown_postprocess.py 대비):
//! - 붙임 표지 표를 `## 붙임: ...` 대신 `[[붙임]] 제목`(dialect::ATTACH)로
//!   되돌린다 — md2hwpx의 실제 붙임 명령이고 title 변형 선택 정보를 보존한다.
//! - 번호 소제목은 마크다운 헤딩 형태로 내보낸다: `N. 제목` 평문 줄은
//!   `## N. 제목`으로, `N.M 제목` 복합 번호 줄은 첫 세그먼트(`N.`)만 벗겨
//!   `### M 제목`으로 승격한다. source.rs가 두 형태를 heading/heading_sub
//!   블록으로 다시 흡수하므로 첨부 안팎 모두 왕복이 고정된다.

use std::sync::LazyLock;

use regex::Regex;

use crate::shared::dialect;

/// 소제목 후보: 80자 이하(바이트가 아니라 문자 수 — 한국어 멀티바이트), 
/// 문장종결 없음, 표 셀 구분자 없음.
const SUBTITLE_MAX_CHARS: usize = 80;
/// 번호 소제목 본문 길이 상한(문자 수).
const NUMBERED_HEADING_MAX: usize = 60;

static SENTENCE_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]。?\s*$").unwrap());
static TABLE_ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\|(.*)\|\s*$").unwrap());
static TABLE_SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\|[\s:|-]+\|\s*$").unwrap());
/// 첨부(붙임) 표지 표: 첫 셀이 붙임/붙 임/붙임 N (내부 공백 무시)
static ATTACH_ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\|\s*(?P<label>붙\s*임\s*\d*)\s*\|(?P<rest>.*)\|\s*$").unwrap());
/// 장표 제목 바 표의 번호 토큰: 1~3단 (`5`, `5-1`, `5.1.2`)
static TITLE_BAR_NUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,2}(?:\s*[-–.]\s*\d{1,2}){0,2}$").unwrap());
/// 번호 소제목: `1. 배 경` — 짧고 문장종결이 없는 번호 줄은 목록이 아니라 섹션 제목
static NUMBERED_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?P<num>\d{1,2})\.\s+(?P<body>\S.*)$").unwrap());
/// 복합 번호 소제목: `2.1 세부` — 앞자리 숫자 세그먼트(`2.`) 뒤에 또 숫자가
/// 오는 줄. 앞 세그먼트들을 한 번에 벗기고 마지막 번호부터 내보낸다
/// (`2.1 세부` → `1 세부`, `2.1.3 예산` → `3 예산`). rest가 `숫자+공백`으로
/// 시작하게 잡아 한 사이클에 고정점으로 수렴시킨다(regex 크레이트는
/// lookahead를 지원하지 않아 "소비하지 않는 lookahead"를 캡처 복사로
/// 구현한다). 이미 벗겨진 `1 세부`(숫자 뒤 공백)는 `숫자.`이 성립하지 않아
/// 재벗기지 않는다. `3.14` 같은 소수는 모양으로 구분할 수 없어 함께 벗겨질
/// 수 있다 — 독립 소제목 줄에서 소수점 표기가 올 확률이 낮아 감수한다.
static COMPOUND_NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\d{1,2}\.)+(?P<rest>\d{1,2}\s\S.*)$").unwrap());

/// 셀 텍스트가 소제목 후보인지 (80자 이하, 문장종결 없음, 파이프 없음).
pub(crate) fn is_subtitle_text(cell: &str) -> bool {
    let cell = cell.trim();
    let n = cell.chars().count();
    0 < n && n <= SUBTITLE_MAX_CHARS && !cell.contains('|') && !SENTENCE_END.is_match(cell)
}

/// 첫 셀이 `붙임`/`붙 임`/`붙임 N`인 표지 표를 `[[붙임]] 제목` 명령 줄로
/// 되돌린다.
///
/// 첨부 문서의 표지 표(붙임 + 제목 셀들)는 데이터 표가 아니므로 헤딩으로
/// 역변환하고, 행과 구분행은 제거한다. 다른 표 규칙보다 먼저 적용해야
/// 3열 표지 표가 잘못 처리되지 않는다. (Python 참조는 `## 붙임: ...` 일반
/// 헤딩으로 풀어냈지만 그러면 md2hwpx의 title 변형 선택 정보가 사라진다.)
pub(crate) fn attachment_table_to_heading(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let matched = ATTACH_ROW.captures(&lines[i]);
        let sep_follows = i + 1 < lines.len() && TABLE_SEP.is_match(&lines[i + 1]);
        if let (Some(m), true) = (matched, sep_follows) {
            let cells: Vec<&str> = m["rest"]
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect();
            let label = m["label"].replace(' ', "");
            let mut parts: Vec<String> = Vec::new();
            match label.strip_prefix("붙임") {
                Some(extra) if !extra.is_empty() => parts.push(extra.to_string()),
                Some(_) => {}
                _ => parts.push(label.clone()),
            }
            parts.extend(cells.iter().map(|s| (*s).to_string()));
            if parts.is_empty() {
                out.push(dialect::ATTACH.to_string());
            } else {
                out.push(format!("{} {}", dialect::ATTACH, parts.join(" ")));
            }
            i += 2;
            continue;
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

/// 1행 표의 셀들이 장표 제목 바 규약에 맞으면 (레벨, 제목)을 반환한다.
///
/// HWPX 파서가 표 배출 형식(파이프 후보 vs 데이터 HTML)을 결정할 때도
/// 같은 판정을 쓴다 — 판정 기준이 배출과 승격에서 갈라지지 않게 한다.
pub(crate) fn title_bar_heading_from_cells(cells: &[String]) -> Option<(usize, String)> {
    let num = cells.first()?.trim();
    if !TITLE_BAR_NUM.is_match(num) {
        return None;
    }
    let title = cells[1..]
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() || !is_subtitle_text(&title) {
        return None;
    }
    let compact: String = num.chars().filter(|c| !c.is_whitespace()).collect();
    let level = compact.split(['-', '–', '.']).count().min(3);
    Some((level, format!("{num} {title}")))
}

/// 장표 제목 바 표(`| 5 |  | 제목 |`)를 헤딩으로 승격한다.
///
/// 1단 번호(`5`)는 문서 제목 바로 `# `로 올리고, 2단 이상(`5-1`, `5.1.2`)은
/// 모두 소제목 정규 형태 `### `로 올린다(번호는 검색 맥락을 위해 제목 앞에
/// 보존). `### ` → heading_sub(소제목 표) → `### ` 경로가 고정점이라 단계
/// 수와 무관하게 왕복이 안정한다. 구분행 뒤에 데이터 행이 이어지는 1행
/// 헤더 데이터 표는 승격 대상에서 제외한다.
pub(crate) fn title_bar_table_to_heading(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let row = TABLE_ROW.captures(&lines[i]);
        let sep_follows = i + 1 < lines.len() && TABLE_SEP.is_match(&lines[i + 1]);
        if let (Some(row), true) = (row, sep_follows) {
            let cells: Vec<String> = row[1].split('|').map(|c| c.trim().to_string()).collect();
            let heading = if cells.is_empty() {
                None
            } else {
                title_bar_heading_from_cells(&cells)
            };
            let has_data_rows = i + 2 < lines.len() && TABLE_ROW.is_match(&lines[i + 2]);
            if let (Some((level, text)), false) = (heading, has_data_rows) {
                let marker = if level >= 2 { "###" } else { "#" };
                out.push(format!("{marker} {text}"));
                i += 2;
                continue;
            }
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

/// 1×1 표(| X | + 구분행)가 소제목 형태면 헤딩으로 역변환한다.
///
/// 문서 맨 앞(아직 비공백 출력이 없을 때)의 1×1 표는 표지 제목이므로 `# `,
/// 이후의 소제목은 `### `로 승격한다 — `### `가 md2hwpx heading_sub(소제목
/// 표)의 정규 형태이고, 셀 텍스트를 그대로 보존해 `### x` → 표 → `### x`
/// 왕복이 고정된다. 실제 데이터 표는 그대로 유지한다.
///
/// 셀 안 복합 번호(`2.1 세부`)도 문단 채널과 같은 규칙으로 앞 세그먼트를
/// 벗겨 `### 1 세부`로 둔다. 채널이 달라도 같은 줄은 같은 결과가 나와야
/// 왕복이 고정되기 때문이다. 벗긴 나머지에 점이 남아도(`2.1.3` → `3`)
/// 한 번에 처리하므로 사이클마다 줄어들지 않는다.
pub(crate) fn subtitle_table_to_heading(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let row = TABLE_ROW.captures(&lines[i]);
        let sep_follows = i + 1 < lines.len() && TABLE_SEP.is_match(&lines[i + 1]);
        if let (Some(row), true) = (row, sep_follows) {
            let cell = row[1].trim().to_string();
            let looks_like_table_continuation =
                i + 2 < lines.len() && TABLE_ROW.is_match(&lines[i + 2]);
            if is_subtitle_text(&cell) && !looks_like_table_continuation {
                let at_document_start = !out.iter().any(|l| !l.trim().is_empty());
                if at_document_start {
                    out.push(format!("# {cell}"));
                } else if let Some(rest) = match_compound_numbered(&cell) {
                    out.push(format!("### {rest}"));
                } else {
                    out.push(format!("### {cell}"));
                }
                i += 2;
                continue;
            }
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

/// `1. 배 경` 형태의 번호 소제목이면 (번호, 본문)을 반환한다.
///
/// 짧고 문장종결이 없는 번호 줄은 본문 목록이 아니라 섹션 제목이다.
pub(crate) fn match_numbered_heading(line: &str) -> Option<(String, String)> {
    let m = NUMBERED_HEADING.captures(line)?;
    let body = m["body"].to_string();
    if body.chars().count() <= NUMBERED_HEADING_MAX && !SENTENCE_END.is_match(&body) {
        Some((m["num"].to_string(), body))
    } else {
        None
    }
}

/// `2.1 세부` 형태의 복합 번호 소제목이면 앞 세그먼트를 벗긴 나머지를
/// 반환한다(`2.1 세부` → `1 세부`, `2.1.3 예산` → `3 예산`). 같은 판정
/// 가드(60자, 문장종결 없음)를 나머지에 적용한다.
pub(crate) fn match_compound_numbered(line: &str) -> Option<String> {
    let m = COMPOUND_NUMBERED.captures(line)?;
    let rest = m["rest"].to_string();
    if rest.chars().count() <= NUMBERED_HEADING_MAX && !SENTENCE_END.is_match(&rest) {
        Some(rest)
    } else {
        None
    }
}

/// 번호 소제목 줄을 마크다운 헤딩으로 승격한다.
///
/// md2hwpx 방언의 번호 헤딩 정규 형태는 마크다운 헤딩이다: `N. 제목` 평문
/// 줄은 `## N. 제목`(source.rs가 heading 블록으로 흡수, 번호 보존)으로,
/// `N.M 제목` 복합 번호 줄은 앞 세그먼트들을 한 번에 벗겨 `### M 제목`
/// (heading_sub)으로 올린다(`2.1 세부` → `### 1 세부`). 승격 결과는 `#`으로 시작해 이 규칙에 재진입하지 않으므로 후처리는
/// 멱등이다. 표 행(`| ... |`)과 `[[...]]` 명령 줄은 번호 모양이어도 그대로
/// 둔다. `## N.` 승격은 문단 줄 채널에만 있고, 소제목 표 채널
/// (subtitle_table_to_heading)에서 온 줄은 `### ` 이미 달려 있어 닿지 않는다.
pub(crate) fn promote_numbered_headings(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if TABLE_ROW.is_match(&line) || line.trim_start().starts_with("[[") {
            out.push(line);
            continue;
        }
        if let Some((num, body)) = match_numbered_heading(&line) {
            out.push(format!("## {num}. {body}"));
        } else if let Some(rest) = match_compound_numbered(&line) {
            out.push(format!("### {rest}"));
        } else {
            out.push(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_length_limit_counts_chars_not_bytes() {
        // 한국어 멀티바이트 경계: 80자는 후보, 81자는 아니다(바이트 수로
        // 세면 80자 한글은 이미 240바이트라 후보에서 탈락한다).
        let k80: String = "가".repeat(80);
        let k81: String = "가".repeat(81);
        assert!(is_subtitle_text(&k80));
        assert!(!is_subtitle_text(&k81));
        assert!(!is_subtitle_text("끝이다."));
        assert!(!is_subtitle_text("a|b"));
    }

    #[test]
    fn attachment_table_becomes_attach_command() {
        let lines: Vec<String> = ["| 붙임 | 별지 제목 |", "| --- | --- |"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            attachment_table_to_heading(lines),
            vec!["[[붙임]] 별지 제목".to_string()]
        );
    }

    #[test]
    fn attachment_numbered_label_is_preserved() {
        let lines: Vec<String> = ["| 붙 임 2 | 별지 제목 |", "| --- | --- |"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            attachment_table_to_heading(lines),
            vec!["[[붙임]] 2 별지 제목".to_string()]
        );
    }

    #[test]
    fn title_bar_table_promotes_by_number_steps() {
        let lines: Vec<String> = [
            "| 5 |  | 현지화 추진현황 및 계획 |",
            "| --- | --- | --- |",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            title_bar_table_to_heading(lines),
            vec!["# 5 현지화 추진현황 및 계획".to_string()]
        );

        let sub: Vec<String> = ["| 5-1 | 하위 제목 |", "| --- | --- |"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            title_bar_table_to_heading(sub),
            vec!["### 5-1 하위 제목".to_string()]
        );

        let deep: Vec<String> = ["| 5.1.2 | 심층 제목 |", "| --- | --- |"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            title_bar_table_to_heading(deep),
            vec!["### 5.1.2 심층 제목".to_string()]
        );
    }

    #[test]
    fn title_bar_table_with_data_rows_is_kept() {
        let lines: Vec<String> = [
            "| 5 | 제목 |",
            "| --- | --- |",
            "| a | b |",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(title_bar_table_to_heading(lines.clone()), lines);
    }

    #[test]
    fn subtitle_table_promotes_with_position_aware_level() {
        // 문서 맨 앞의 1×1 소제목 표는 표지 제목(#), 이후는 소제목(###).
        let first: Vec<String> = ["| 보고서 제목 |", "| --- |"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            subtitle_table_to_heading(first),
            vec!["# 보고서 제목".to_string()]
        );

        let later: Vec<String> = vec![
            "앞 문단".to_string(),
            "| 2.1 개요 |".to_string(),
            "| --- |".to_string(),
        ];
        assert_eq!(
            subtitle_table_to_heading(later),
            vec![
                "앞 문단".to_string(),
                "### 1 개요".to_string()
            ]
        );
    }

    #[test]
    fn subtitle_table_strips_compound_cell_like_paragraph_channel() {
        // 표 채널도 문단 채널과 같은 규칙으로 앞 세그먼트를 벗긴다 —
        // 채널이 달라도 같은 줄은 같은 `### ` 결과가 나와야 왕복이 고정된다.
        let lines: Vec<String> = vec![
            "앞 문단".to_string(),
            "| 1.2 유지 |".to_string(),
            "| --- |".to_string(),
        ];
        assert_eq!(
            subtitle_table_to_heading(lines),
            vec!["앞 문단".to_string(), "### 2 유지".to_string()]
        );
    }

    #[test]
    fn numbered_headings_use_source_dialect_tokens() {
        // 번호 소제목은 첨부 안팎 모두 `## N. 제목` 마크다운 헤딩으로
        // 승격한다. `1.  배 경`처럼 공백이 밀려도 정규화된다.
        let plain: Vec<String> = vec!["1.  배 경".to_string(), "본문".to_string()];
        assert_eq!(
            promote_numbered_headings(plain),
            vec!["## 1. 배 경".to_string(), "본문".to_string()]
        );

        let in_attach: Vec<String> =
            vec!["[[붙임]] 별지".to_string(), "2. 방 안".to_string()];
        assert_eq!(
            promote_numbered_headings(in_attach),
            vec![
                "[[붙임]] 별지".to_string(),
                "## 2. 방 안".to_string()
            ]
        );
    }

    #[test]
    fn compound_numbered_line_strips_leading_segments_at_once() {
        // `2.1 세부` → 앞 `2.`만 벗겨 `### 1 세부`. 세그먼트가 더 남으면
        // (`2.1.3`) 한 번에 처리해 `### 3 예산`으로 바로 고정점에 둔다.
        // 뒤의 숫자는 rest에 포함해 그대로 살린다.
        let lines: Vec<String> = vec![
            "2.1 세부".to_string(),
            "10.2 계획".to_string(),
            "2.1.3 예산".to_string(),
        ];
        assert_eq!(
            promote_numbered_headings(lines),
            vec![
                "### 1 세부".to_string(),
                "### 2 계획".to_string(),
                "### 3 예산".to_string()
            ]
        );
    }

    #[test]
    fn stripped_or_marked_lines_are_not_restripped() {
        // 이미 벗겨진 `1 세부`(숫자 뒤 공백)와 승격된 `## `/`### ` 줄은
        // 그대로 둔다 — 후처리 멱등성.
        let lines: Vec<String> = vec![
            "1 세부".to_string(),
            "## 1. 제목".to_string(),
            "### 1 세부".to_string(),
            "| 2.1 표 행 |".to_string(),
            "| --- |".to_string(),
            "[[붙임]] 2.1 문서".to_string(),
        ];
        assert_eq!(promote_numbered_headings(lines.clone()), lines);
    }

    #[test]
    fn compound_guards_reject_sentences_and_long_rests() {
        assert!(match_compound_numbered("2.1 다음과 같다.").is_none());
        // rest는 `숫자+공백`으로 시작해야 한다. 길이 상한은 rest 기준 60자다.
        assert!(match_compound_numbered(&format!("2.1 {}", "가".repeat(57))).is_some());
        assert!(match_compound_numbered(&format!("2.1 {}", "가".repeat(59))).is_none());
    }

    #[test]
    fn promoted_headings_survive_a_second_postprocess_pass() {
        // parse-then-parse 안정성: 승격 결과에 후처리를 다시 돌려도 같다.
        let raw = "# 표지\n\n1. 개요\n\n2.1 세부\n\n본문";
        let once = crate::parse::postprocess::postprocess_markdown(raw);
        assert!(once.contains("## 1. 개요"));
        assert!(once.contains("### 1 세부"));
        assert_eq!(
            crate::parse::postprocess::postprocess_markdown(&once),
            once
        );
    }

    #[test]
    fn numbered_heading_length_limit_counts_chars_not_bytes() {
        let body60 = "가".repeat(60);
        let body61 = "가".repeat(61);
        assert!(match_numbered_heading(&format!("1. {body60}")).is_some());
        assert!(match_numbered_heading(&format!("1. {body61}")).is_none());
    }

    #[test]
    fn sentence_like_numbered_lines_are_not_promoted() {
        assert!(match_numbered_heading("1. 다음과 같다.").is_none());
        assert!(match_numbered_heading("1. 아주 긴 본문".to_string().as_str()).is_some());
    }
}
