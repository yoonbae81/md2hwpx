//! 표→헤딩 승격 규칙과, 표 배출(parse.rs)과 승격이 같이 쓰는 판정 가드.
//!
//! "같은 판정이 배출과 승격에서 갈라지지 않게" 하는 계약을 구조적으로
//! 지키기 위해 `is_subtitle_text`/`title_bar_heading_from_cells`는 이
//! 모듈에만 정의하고 parse.rs가 import한다.
//!
//! 왕복 편차(Python 참조 markdown_postprocess.py 대비):
//! - 붙임 표지 표를 `## 붙임: ...` 대신 `[[붙임]] 제목`(dialect::ATTACH)으로
//!   되돌린다 — md2hwpx의 실제 붙임 명령이고 title 변형 선택 정보를 보존한다.
//! - 번호 소제목 승격 토큰은 실측으로 확정했다: `N. 제목` 평형(source.rs
//!   NUM_RE → 전용 heading 블록, 번호 보존 확인)이 유일한 안정 형태라 첨부
//!   안팎 모두 평형으로 내보낸다. `## `/`### ` 마크다운 헤딩(heading_default)
//!   렌더는 ` x` 평문이 되어 재흡수 시 불릿으로 퇴화한다.

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

/// 장표 제목 바 표(`| 5 |  | 제목 |`)를 번호 단계 수의 헤딩으로 승격한다.
///
/// 번호 단계 수(`5`=1단, `5-1`=2단)가 헤딩 레벨이 되고, 번호는 검색 맥락을
/// 위해 제목 앞에 보존된다. 구분행 뒤에 데이터 행이 이어지는 1행 헤더 데이터
/// 표는 승격 대상에서 제외한다.
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
                out.push(format!("{} {}", "#".repeat(level), text));
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
/// 이후의 소제목은 `## `로 승격한다. 실제 데이터 표는 그대로 유지한다.
///
/// 잔여 한계(실측, 되돌릴 수 없음): md2hwpx의 heading_sub 렌더(render.rs
/// SUB_NUM_PREFIX_RE)는 `2.1` 같은 번호를 컴파일 시점에 벗겨 내므로
/// 번호 없는 1×1 표는 heading_sub로 재진입할 수 없다. `## x`(heading_default)
/// 렌더도 ` x` 평문이라 재인식되지 않아, md2hwpx로 두 번 더 돌리면
/// `- x` 불릿에 수렴한다. 1열 파이프 표·HTML은 try_table 거부로 즉시
/// 쓰레기 불릿이 되므로 더 나쁘고, 번호를 지어내는 것보다 `## `이 충실하다.
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
                let prefix = if at_document_start { "# " } else { "## " };
                out.push(format!("{prefix}{cell}"));
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

/// `1. 배 경` 형태의 번호 소제목을 헤딩으로 승격한다.
///
/// md2hwpx 방언에서 번호 헤딩의 안정 형태는 `N. 제목` 평형 하나뿐이다 —
/// 전용 heading 블록(NUM_RE)은 번호를 보존해 왕복이 고정되지만, `### `
/// 마크다운 헤딩(heading_default) 렌더는 ` x` 평문으로 나와 재흡수 시
/// 불릿으로 퇴화한다(실측). 그래서 첨부([[붙임]]) 안팎 모두 평형으로
/// 내보낸다. `N.M` 형식 소제목은 source.rs가 직접 heading_sub로 분류한다.
pub(crate) fn promote_numbered_headings(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if let Some((num, body)) = match_numbered_heading(&line) {
            out.push(format!("{num}. {body}"));
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
            vec!["## 5-1 하위 제목".to_string()]
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
        // 문서 맨 앞의 1×1 소제목 표는 표지 제목(#), 이후는 소제목(##).
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
                "## 2.1 개요".to_string()
            ]
        );
    }

    #[test]
    fn numbered_headings_use_source_dialect_tokens() {
        // 번호 소제목은 첨부 안팎 모두 `N. 제목` 평형(번호 보존 확인된 유일한
        // 안정 형태)으로 정규화한다. `1.  배 경`처럼 공백이 밀려도 정규화된다.
        let plain: Vec<String> = vec!["1.  배 경".to_string(), "본문".to_string()];
        assert_eq!(
            promote_numbered_headings(plain),
            vec!["1. 배 경".to_string(), "본문".to_string()]
        );

        let in_attach: Vec<String> =
            vec!["[[붙임]] 별지".to_string(), "2. 방 안".to_string()];
        assert_eq!(
            promote_numbered_headings(in_attach),
            vec![
                "[[붙임]] 별지".to_string(),
                "2. 방 안".to_string()
            ]
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
