//! 마지막 빈 줄 정규화(컴팩션) — 후처리 파이프라인의 최종 단계.
//!
//! `hwpx_to_markdown_bytes`는 블록을 빈 줄로 연결하고(`join("\n\n")`), 표
//! 블록은 앞뒤에 개행을 달고 나오므로, 규칙을 거친 출력에는 블록 사이마다
//! 1~3칸의 빈 줄이 흩어진다. 이 규칙이 문서를 읽기 좋은 정규 형태로
//! 끌어간다:
//!
//! - 세그먼트 앞뒤 빈 줄 제거. 단, 입력이 빈 줄로 끝났다면(=종료 개행)
//!   정확히 한 칸만 유지한다 — 마지막 블록이 표일 때 출력이 `\n`으로
//!   끝나는 기존 종료 의미를 바꾸지 않는다.
//! - 줄을 분류해 런(run)을 이룬다: 불릿/참고(`※`, `* `) 줄, 표 줄,
//!   하이라이트(여는 `---`부터 닫는 `---`까지), 박스(`[[...]]` 줄 + 뒤따르는
//!   불릿), 문단 줄. 런 내부의 빈 줄은 지운다(표 내부의 빈 줄은 컴파일러의
//!   `try_table`이 표 흡수를 끊는 지점이므로 왕복 보존에도 필요하다).
//! - 런 사이는 정확히 한 칸으로 맞춘다. 헤딩·명령·표 시작·`---` 줄 앞에는
//!   빈 줄이 없으면 한 칸 보충한다(하이라이트 내부는 제외 — 경계와 항목은
//!   붙어 있어야 컴파일러의 5줄 창 페어링이 그대로 성립한다).
//!
//! 컴파일러(source.rs)는 빈 줄을 건너뛰고 블록 판정은 줄 모양으로 하므로
//! 이 규칙은 컴파일 결과를 바꾸지 않는다(아래
//! `compacted_parse_output_compiles_identically_to_inflated_form` 참조).
//! ``` 코드 펜스 내부는 `apply_outside_code`가 이미 보호한다.

use crate::shared::dialect;
use crate::shared::glyph;

/// 빈 줄 정규화의 대상이 되는 줄 종류. 판정은 항상 trimmed 텍스트 기준.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineClass {
    Blank,
    /// `---` — 하이라이트 경계 또는 가로선.
    Fence,
    /// `| ... |` 파이프 표 행(heading.rs TABLE_ROW 모양).
    Table,
    /// `[[박스]]`/`[[붙임]]` 등 명령 줄.
    Command,
    /// `# ` 마크다운 헤딩 또는 `N. `/`N.M ` 번호 소제목 모양.
    Heading,
    /// `- `/`* `/`+ ` 또는 글리프·`※`로 시작하는 항목/참고 줄(들여쓰기 무관).
    Bullet,
    /// 그 외 본문 줄.
    Para,
}

fn classify_line(line: &str) -> LineClass {
    let t = line.trim();
    if t.is_empty() {
        return LineClass::Blank;
    }
    if t == dialect::HRULE {
        return LineClass::Fence;
    }
    if t.len() >= 2 && t.starts_with('|') && t.ends_with('|') {
        return LineClass::Table;
    }
    if t.starts_with("[[") {
        return LineClass::Command;
    }
    if is_heading_shape(t) {
        return LineClass::Heading;
    }
    if is_bullet_shape(line) {
        return LineClass::Bullet;
    }
    LineClass::Para
}

/// `#{1,6} ` 마크다운 헤딩 또는 번호 소제목 모양인지.
fn is_heading_shape(t: &str) -> bool {
    let hashes = t.chars().take_while(|&c| c == '#').count();
    if hashes > 0 {
        return hashes <= 6 && t[hashes..].starts_with(char::is_whitespace);
    }
    is_numbered_heading_shape(t)
}

/// `N. 제목`(heading.rs NUMBERED_HEADING) 또는 `N.M 제목`
/// (COMPOUND_NUMBERED) 모양인지. 모양만 본다(길이·문장종결 가드는 승격
/// 규칙의 판정이고, 여기서는 빈 줄 배치에만 쓴다). 세 자리 이상 숫자는
/// 두 regex 모두 매치에 실패한다.
fn is_numbered_heading_shape(t: &str) -> bool {
    let mut rest = t;
    let mut dots = 0;
    while let Some(after) = strip_number_dot(rest) {
        rest = after;
        dots += 1;
    }
    if dots == 0 {
        return false;
    }
    // `N. 제목`: 마지막 점 바로 뒤에 공백 1+ 이후 본문이 온다.
    if rest.starts_with(char::is_whitespace) {
        return !rest.trim_start_matches(char::is_whitespace).is_empty();
    }
    // `N.M 제목`: `N.` 세그먼트들 뒤 숫자 1~2자리 + 공백 한 칸 + 본문.
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 2 {
        return false;
    }
    let tail = &rest[digits..];
    let mut chars = tail.chars();
    match chars.next() {
        Some(ws) if ws.is_whitespace() => {
            let body = &tail[ws.len_utf8()..];
            !body.is_empty() && !body.starts_with(char::is_whitespace)
        }
        _ => false,
    }
}

/// 최대 두 자리 숫자와 점으로 시작하면 점 뒤를 반환한다.
fn strip_number_dot(t: &str) -> Option<&str> {
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 2 {
        return None;
    }
    t[digits..].strip_prefix('.')
}

/// ASCII 불릿(-, *, +)은 뒤에 공백이 따라와야 목록이다(`**굵게**` 표식과
/// 구별 — source.rs bullet_split과 같은 기준). 글리프 불릿은 붙여 써도
/// 목록이다(bullets.rs parse_bullet_line과 같은 기준). 참고 표기 `※`와
/// 원문자 ⓪도 항목 줄로 본다.
fn is_bullet_shape(line: &str) -> bool {
    let s = line.trim_start();
    let Some(c) = s.chars().next() else {
        return false;
    };
    if matches!(c, '-' | '*' | '+') {
        return s[c.len_utf8()..].starts_with(char::is_whitespace);
    }
    glyph::is_bullet_glyph(c) || c == '※' || c == '⓪'
}

/// 현재 줄 앞에 빈 줄이 있어야 하는지(강제 삽입 / 있으면 유지 / 없음).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gap {
    /// 런 내부(하이라이트 내부, 연속 불릿/표/문단, 박스 항목) — 빈 줄을 지운다.
    None,
    /// 런 사이 — 이미 있던 빈 줄만 한 칸 유지한다.
    Keep,
    /// 헤딩·명령·표 시작·`---` 앞 — 없으면 한 칸 삽입한다.
    Force,
}

fn gap_decision(cur: LineClass, prev: Option<LineClass>, inside_highlight: bool) -> Gap {
    if inside_highlight {
        return Gap::None;
    }
    let Some(prev) = prev else {
        return Gap::None;
    };
    match cur {
        LineClass::Heading | LineClass::Command | LineClass::Fence => Gap::Force,
        LineClass::Table => {
            if prev == LineClass::Table {
                Gap::None
            } else {
                Gap::Force
            }
        }
        // 불릿은 앞 불릿·박스 명령과 붙는다(박스 항목 계약: 명령 줄보다 깊은
        // 들여쓰기의 불릿은 상자에 속한다).
        LineClass::Bullet => match prev {
            LineClass::Bullet | LineClass::Command => Gap::None,
            _ => Gap::Keep,
        },
        LineClass::Para => {
            if prev == LineClass::Para {
                Gap::None
            } else {
                Gap::Keep
            }
        }
        LineClass::Blank => Gap::None,
    }
}

/// 하이라이트 구간(페어링된 `---` 쌍, 닫는 경계 포함 [open, close]).
///
/// 페어링은 컴파일러(source.rs)와 같은 탐욕 규칙이다 — 닫는 `---`가 5줄
/// 창 안에 있고 사이에 내용이 있어야 경계 쌍이고, 문서 첫 비공백 줄의
/// `---`는 가로선/front matter 전용으로 제외한다. 여기서 빈 줄을 넣거나
/// 지워도 경계 간 거리는 컴파일러 페어링 결과를 바꾸지 않는다(경계 사이
/// 빈 줄은 지우기만 하고, 삽입은 경계 바깥에만 한다).
struct HighlightSpans {
    spans: Vec<(usize, usize)>,
}

impl HighlightSpans {
    fn new(classes: &[LineClass]) -> Self {
        const WINDOW: usize = 5; // source.rs HIGHLIGHT_WINDOW와 같다.
        let first_nonblank = classes.iter().position(|c| *c != LineClass::Blank);
        let fences: Vec<usize> = (0..classes.len())
            .filter(|&i| classes[i] == LineClass::Fence && Some(i) != first_nonblank)
            .collect();
        let mut spans = Vec::new();
        let mut pair = 0;
        while pair + 1 < fences.len() {
            let (open, close) = (fences[pair], fences[pair + 1]);
            let within_window = close - open <= WINDOW;
            let has_content = (open + 1..close).any(|j| classes[j] != LineClass::Blank);
            if within_window && has_content {
                spans.push((open, close));
                pair += 2;
            } else {
                pair += 1;
            }
        }
        Self { spans }
    }

    /// 하이라이트 경계 안쪽(여는 `---` 제외, 닫는 `---` 포함)인지 — 이
    /// 범위에서는 빈 줄을 넣지도 유지하지도 않는다.
    fn contains_interior(&self, i: usize) -> bool {
        self.spans.iter().any(|&(open, close)| open < i && i <= close)
    }
}

/// 빈 줄을 정규 형태로 컴팩션한다: 앞뒤 빈 줄 제거(종료 개행 한 칸은
/// 유지), 런 내부 빈 줄 삭제, 런 사이 정확히 한 칸, 헤딩·명령·표 시작·
/// `---` 앞 한 칸 보충. 비(非)빈 줄의 순서와 내용은 건드리지 않는다.
pub(crate) fn compact_blank_lines(lines: Vec<String>) -> Vec<String> {
    let classes: Vec<LineClass> = lines.iter().map(|l| classify_line(l)).collect();
    let highlight = HighlightSpans::new(&classes);
    // 종료 개행 보존: 입력이 빈 줄로 끝나면(마지막 블록이 표일 때의
    // `\n` 종료) 출력도 정확히 한 칸의 빈 줄로 끝난다.
    let ends_with_blank = classes.last().is_some_and(|c| *c == LineClass::Blank);
    let mut out: Vec<String> = Vec::new();
    let mut prev: Option<LineClass> = None;
    let mut gap_pending = false;
    for (i, line) in lines.into_iter().enumerate() {
        if classes[i] == LineClass::Blank {
            gap_pending = true;
            continue;
        }
        let decision = gap_decision(classes[i], prev, highlight.contains_interior(i));
        let blank_before = match decision {
            Gap::Force => prev.is_some(),
            Gap::Keep => gap_pending && prev.is_some(),
            Gap::None => false,
        };
        if blank_before {
            out.push(String::new());
        }
        out.push(line);
        prev = Some(classes[i]);
        gap_pending = false;
    }
    if ends_with_blank && !out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn compact(v: &[&str]) -> Vec<String> {
        compact_blank_lines(lines(v))
    }

    /// 사용자 실제 불만 사례: 모든 블록이 빈 줄(더블 포함)로 떨어져 있는
    /// 문서가 정규 형태로 컴팩션되는지.
    #[test]
    fn doubly_spaced_document_compacts_to_normal_form() {
        let src = [
            "",
            "",
            "# 보고서 제목",
            "",
            "",
            "## 1. 개 요",
            "",
            "",
            "- (배경) 첫 항목",
            "",
            "",
            "  - (현황) 하위 항목",
            "",
            " ※ 참고 사항",
            "",
            "## 2. 주요내용",
            "",
            "",
            "---",
            "",
            "- 핵심 요약 문장",
            "",
            "---",
            "",
            "",
            "[[박스]] 기대 효과",
            "",
            "  - 첫 항목",
            "",
            "",
            "  - 둘째 항목",
            "",
            "",
            "| 구분 | 결과 |",
            "| --- | --- |",
            "",
            "| a | b |",
            "",
            "",
            "본문 문단",
        ];
        assert_eq!(
            compact(&src),
            lines(&[
                "# 보고서 제목",
                "",
                "## 1. 개 요",
                "",
                "- (배경) 첫 항목",
                "  - (현황) 하위 항목",
                " ※ 참고 사항",
                "",
                "## 2. 주요내용",
                "",
                "---",
                "- 핵심 요약 문장",
                "---",
                "",
                "[[박스]] 기대 효과",
                "  - 첫 항목",
                "  - 둘째 항목",
                "",
                "| 구분 | 결과 |",
                "| --- | --- |",
                "| a | b |",
                "",
                "본문 문단",
            ])
        );
    }

    /// 표 내부의 빈 줄은 컴파일러 `try_table`이 표 흡수를 끊는 지점이다 —
    /// 런 내부 빈 줄 삭제가 왕복 보존에 필요하다.
    #[test]
    fn blank_inside_table_run_is_deleted() {
        assert_eq!(
            compact(&["| a | b |", "", "| --- | --- |", "", "", "| 1 | 2 |"]),
            lines(&["| a | b |", "| --- | --- |", "| 1 | 2 |"])
        );
    }

    /// 박스 명령 + 항목은 붙어 있고, `## ` 바로 아래의 여는 `---` 앞에는
    /// 정확히 한 칸이 온다.
    #[test]
    fn box_stays_contiguous_and_highlight_open_gets_one_blank() {
        assert_eq!(
            compact(&["## 2. 주요내용", "---", "- 요약", "---", "[[박스]] 제목", "  - 항목",]),
            lines(&[
                "## 2. 주요내용",
                "",
                "---",
                "- 요약",
                "---",
                "",
                "[[박스]] 제목",
                "  - 항목",
            ])
        );
    }

    /// 문서 앞뒤 빈 줄은 지우되, 종료 개행(빈 줄로 끝나는 입력)은 정확히
    /// 한 칸 유지한다 — 마지막 블록이 표인 문서의 기존 종료 의미.
    #[test]
    fn trailing_newline_semantics_is_preserved() {
        // 표로 끝나는 입력(종료 개행 있음) → 종료 개행 유지.
        assert_eq!(
            compact(&["| a | b |", "| --- | --- |", ""]),
            lines(&["| a | b |", "| --- | --- |", ""])
        );
        // 문단으로 끝나는 입력(종료 개행 없음) → 없는 채로 둔다.
        assert_eq!(compact(&["본문 문단"]), lines(&["본문 문단"]));
        // 종료 빈 줄이 여러 칸이어도 한 칸으로 정규화된다.
        assert_eq!(compact(&["본문 문단", "", "", ""]), lines(&["본문 문단", ""]));
        // 앞쪽 빈 줄은 무조건 지운다.
        assert_eq!(compact(&["", "", "# 제목", "", "본문"]), lines(&["# 제목", "", "본문"]));
    }

    /// 멱등성: 컴팩션 결과에 다시 컴팩션을 돌려도 같다.
    #[test]
    fn compaction_is_idempotent() {
        let samples: Vec<Vec<&str>> = vec![
            vec!["# 제목", "", "", "## 1. 개 요", "", "- 항목", "  - 하위", "", "", "본문"],
            vec!["---", "- a", "- b", "---", "", "---", "- c", "---"],
            vec!["| a | b |", "", "| --- | --- |", "| 1 | 2 |", ""],
            vec!["[[박스]] x", "  - a", "", "  - b", "", "", "[[붙임]] y", "", "# z"],
            vec!["", "", ""],
            vec!["본문 뿐"],
        ];
        for src in samples {
            let once = compact(&src);
            let twice = compact_blank_lines(once.clone());
            assert_eq!(twice, once, "src: {src:?}");
        }
    }

    /// 번호 소제목 모양(`1. 개요`, `2.1 세부`)의 평문 줄도 헤딩으로 보아
    /// 앞에 한 칸을 보충한다(승격 규칙이 이미 `## `/`### `를 달았지만,
    /// 멱등 재입력에서는 평문 형태로 올 수 있다).
    #[test]
    fn bare_numbered_heading_shapes_get_a_blank_before() {
        assert_eq!(
            compact(&["- 항목", "1. 개요", "본문", "2.1 세부", "- 다음"]),
            lines(&["- 항목", "", "1. 개요", "본문", "", "2.1 세부", "- 다음"])
        );
    }

    /// 홀로 있는 `---`(가로선)은 앞에 한 칸을 보충해 독립 줄로 둔다.
    /// 사이에 빈 줄만 있는 `---` 두 줄은 하이라이트가 아니라 가로선 두 개다
    /// (5줄 창 + 내용 조건). 창 안에서 내용을 낀 `---` 쌍은 하이라이트 런이라
    /// 경계와 항목이 붙는다(컴파일러 페어링과 같은 판정).
    #[test]
    fn unpaired_dashes_stay_separated_rules() {
        assert_eq!(
            compact(&["본문", "---", "본문2"]),
            lines(&["본문", "", "---", "본문2"])
        );
        assert_eq!(
            compact(&["본문", "---", "", "---", "본문2"]),
            lines(&["본문", "", "---", "", "---", "본문2"])
        );
        // 창 안에서 내용을 낀 `---` 쌍은 하이라이트 런 — 경계와 항목이 붙는다
        // (컴파일러 페어링과 같은 판정). 닫는 `---` 뒤 빈 줄은 삽입 대상이
        // 아니라 있던 것만 유지된다(파이프라인 출력은 블록 결합으로 항상
        // 한 칸이 이미 있다).
        assert_eq!(
            compact(&["본문", "---", "요약", "---", "본문2"]),
            lines(&["본문", "", "---", "요약", "---", "본문2"])
        );
        assert_eq!(
            compact(&["본문", "---", "요약", "---", "", "본문2"]),
            lines(&["본문", "", "---", "요약", "---", "", "본문2"])
        );
    }

    /// ``` 코드 펜스: 펜스 줄과 내부는 파이프라인 전체가 건드리지 않는다.
    /// 여는 펜스 앞 빈 줄은 한 칸 유지되고, 닫는 펜스 뒤 빈 줄은 세그먼트
    /// 맨 앞 빈 줄 제거(문서 가장자리 정리)의 대상이 되어 붙는다 — 컴파일
    /// 결과는 불변이다.
    #[test]
    fn code_fences_and_their_interiors_survive_the_pipeline() {
        let markdown = "문단\n\n```\n내부\n\n빈 줄도 유지\n```\n\n뒤 문단";
        let out = crate::parse::postprocess::postprocess_markdown(markdown);
        assert_eq!(
            out,
            "문단\n\n```\n내부\n\n빈 줄도 유지\n```\n뒤 문단",
            "out:\n{out}"
        );
    }

    /// 파이프라인 전체(마지막 규칙 포함)를 두 번 돌려도 같다 — 실사용
    /// 재입력 안정성.
    #[test]
    fn full_pipeline_is_idempotent_on_compacted_sample() {
        let raw = "\
# 표지

1. 개요

- 항목

2.1 세부

본문";
        let once = crate::parse::postprocess::postprocess_markdown(raw);
        let twice = crate::parse::postprocess::postprocess_markdown(&once);
        assert_eq!(twice, once, "once:\n{once}");
        assert!(once.contains("## 1. 개요"));
        assert!(once.contains("### 1 세부"));
    }

    /// 컴파일 동치: 이 규칙은 컴파일 결과를 바꾸지 않는다. 정규 형태(컴팩션
    /// 적용)와 빈 줄을 부풀린 변형(컴팩션 전 파이프라인이 내놓던 넉넉한
    /// 배치와 같은 부류)이 같은 HWPX를 낳는다 — 빈 줄 배치가 의미 없음의
    /// 증명이다. (역변환 출력과 원본 example.md의 컴파일은 볼드 마커가
    /// 역방향에서 유실되는 별개의 기존 동작이라 비교 대상이 아니다.)
    /// 표지 날짜는 컴파일 시점에 찍히므로 같은 실행 내 비교다(lib.rs 왕복
    /// 테스트와 같은 방식).
    #[test]
    fn compacted_parse_output_compiles_identically_to_inflated_form() {
        use crate::{compile_hwpx_bytes, hwpx_to_markdown_bytes};

        static TEMPLATE: &[u8] = include_bytes!("../../../web/public/template.hwpx");
        let example = include_str!("../../../web/public/example.md");
        let hwpx_orig = compile_hwpx_bytes(example, TEMPLATE, &[]).unwrap().0;
        let (md, _) = hwpx_to_markdown_bytes(&hwpx_orig, "roundtrip.hwpx").unwrap();

        // 정규 형태 확인: 빈 줄 런은 모두 정확히 한 칸.
        let mut prev_blank = false;
        for line in md.split('\n') {
            let blank = line.trim().is_empty();
            assert!(
                !(blank && prev_blank),
                "연속된 빈 줄이 남았다: {md:?}"
            );
            prev_blank = blank;
        }

        // 컴팩션된 출력은 다시 역변환해도 그대로다(왕복 고정점).
        let (md_again, _) = hwpx_to_markdown_bytes(
            &compile_hwpx_bytes(&md, TEMPLATE, &[]).unwrap().0,
            "roundtrip2.hwpx",
        )
        .unwrap();
        assert_eq!(md_again, md, "md:\n{md}");

        // 빈 줄 배치만 다른 변형(런 사이 빈 줄 2칸 + 문서 가장자리 빈 줄)도
        // 같은 HWPX를 낳는다. 하이라이트 내부에는 빈 줄이 없으므로(위에서
        // 지웠다) 부풀림이 경계 페어링을 건드리지 않는다.
        let inflated = inflate_blank_runs(&md);
        let hwpx_compact = compile_hwpx_bytes(&md, TEMPLATE, &[]).unwrap().0;
        let hwpx_inflated = compile_hwpx_bytes(&inflated, TEMPLATE, &[]).unwrap().0;
        assert_eq!(hwpx_inflated, hwpx_compact);
    }

    /// 런 사이 빈 줄을 두 칸으로 부풀리고 문서 가장자리에 빈 줄을 더한
    /// 변형을 만든다(컴파일 불변성 증명용).
    fn inflate_blank_runs(md: &str) -> String {
        let mut out: Vec<&str> = vec!["", ""];
        let mut in_blank = false;
        for line in md.split('\n') {
            if line.trim().is_empty() {
                if !in_blank {
                    out.push("");
                    out.push("");
                }
                in_blank = true;
            } else {
                out.push(line);
                in_blank = false;
            }
        }
        if !in_blank {
            out.push("");
            out.push("");
        }
        out.join("\n")
    }

    /// 마지막 블록이 표인 문서의 파이프라인 출력은 개행 하나로 끝난다
    /// (paragraph_to_markdown가 표 뒤에 `\n`을 붙인다). 컴팩션이 이 종료
    /// 개행을 지우지 않는다.
    #[test]
    fn pipeline_output_for_table_last_document_keeps_single_trailing_newline() {
        use crate::parse::hwpx::test_fixtures::fixture_hwpx;
        use crate::hwpx_to_markdown_bytes;

        let section = "\
<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<hp:hs xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\">\
<hp:p><hp:run><hp:t>제목</hp:t></hp:run></hp:p>\
<hp:p><hp:tbl rowCnt=\"2\"><hp:tr>\
<hp:tc><hp:p><hp:run><hp:t>구분</hp:t></hp:run></hp:p></hp:tc>\
<hp:tc><hp:p><hp:run><hp:t>결과</hp:t></hp:run></hp:p></hp:tc>\
</hp:tr><hp:tr>\
<hp:tc><hp:p><hp:run><hp:t>빌드</hp:t></hp:run></hp:p></hp:tc>\
<hp:tc><hp:p><hp:run><hp:t>성공</hp:t></hp:run></hp:p></hp:tc>\
</hp:tr></hp:tbl></hp:p>\
</hp:hs>";
        let bytes = fixture_hwpx(&[("Contents/section0.xml", section)]);
        let (markdown, _) = hwpx_to_markdown_bytes(&bytes, "table-last.hwpx").unwrap();
        assert!(markdown.ends_with('\n'), "md:\n{markdown}");
        assert!(!markdown.ends_with("\n\n"), "md:\n{markdown}");
        assert!(!markdown.starts_with('\n'), "md:\n{markdown}");
    }
}
