//! HWPX(HWPML) 네이티브 파싱 코어 — retriever hwpx.py의 직접 포트.
//!
//! HWPX 파일 구조 지식을 역방향으로 적용한다:
//! - HWPX = ZIP + Contents/section*.xml (Hancom HWPML)
//! - hp:p(문단)/hp:run(런)/hp:t(텍스트)/hp:lineBreak(단락 내 줄바꿈)/hp:tbl(표)/hp:ctrl(필드·메모)
//! - hp:t는 자식 요소(hp:fwSpace 등)의 tail까지 추출한다 — 글머리표(ㅇ)와 들여쓰기 공백이
//!   tail에만 존재하는 문단이 있어 t.text 단독 수집은 불릿과 날짜 필드 텍스트를 유실한다
//! - 문단 수집(본문·표 셀 공용)은 hp:tbl 내부로 내려가지 않는다 — 표 안 문단이
//!   표와 별개로 재출력(이중 출력)되는 것을 원천 차단한다. 표 자체는 문단을
//!   순회하는 도중(`paragraph_parts`)에 만나면 그 자리에서 직렬화하므로, 셀 안에
//!   또 다른 표가 있어도 재귀적으로 올바르게 중첩된다
//! - MEMO 필드 범위(fieldBegin type=MEMO → fieldEnd)는 본문·표 셀 공통으로 제외한다
//! - hp:lineBreak(Shift+Enter)는 저자의 의도적 소프트 줄바꿈으로 보존한다 —
//!   시각적 wrap과 구분할 수 없는 출력이 아니므로 계속줄 결합을 하지 않는다
//! - 표 배출 정책(마크다운 후처리와의 계약): 헤딩 후보 표(1×1 소제목, 장표
//!   제목 바)는 파이프 표로 배출해 공용 후처리가 승격하게 하고, 그 외 데이터
//!   표는 colSpan/rowSpan을 보존한 HTML <table>로 직렬화한다. 후보 판정 가드
//!   (`is_subtitle_text`, `title_bar_heading_from_cells`)는 배출과 승격이 같은
//!   기준을 쓰도록 postprocess::heading에서 import한다
//! - 문서 관습 복원(표→헤딩, 불릿 계층, 박스 등)은 이 모듈이 하지 않고
//!   공용 후처리(`postprocess::postprocess_markdown`)에 위임한다

use crate::parse::postprocess::heading::{is_subtitle_text, title_bar_heading_from_cells};
use crate::shared::xmltree::{find_child, find_children, HP, NodeRef};

/// 문단을 구성하는 파트. Python의 (kind, content) 튜플 대신 타입으로 구분한다.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Br,
    /// 그 자리에서 직렬화된 표 조각(파이프 마크다운 또는 HTML).
    Table(String),
}

/// 파싱 중 집계되는 통계와 경고. 경고는 Report로 나가 종료 코드 1의 근거가
/// 된다(예: 손상된 표 복구).
#[derive(Default)]
pub struct ParseStats {
    pub warnings: Vec<String>,
    pub pipe_tables: usize,
    pub html_tables: usize,
}

/// `hp:t` 하나를 파트로 분해한다.
///
/// hp:t의 표시 텍스트는 자식 요소(hp:fwSpace 등)의 tail까지 포함해야 한다 —
/// 글머리표·들여쓰기 공백이 tail에만 존재하는 문단이 있어 t.text만 보면
/// 유실된다. 다만 자식으로 hp:lineBreak가 중첩된 경우(실측: 줄바꿈이 run
/// 경계가 아니라 t 내부에 있는 문서가 있다)는 단순히 이어붙이면 줄바꿈의
/// 실제 위치가 사라지고 뒤 텍스트가 앞 텍스트에 붙어버린다 — 그 지점에서
/// text/br/text로 쪼갠다. fwSpace처럼 lineBreak가 아닌 자식은 그대로
/// tail까지 이어붙인다. (fwSpace 자체는 xmltree::parse가 일반 공백으로
/// 흡수하므로 여기까지는 보이지 않는다.)
pub fn t_parts(t: &NodeRef) -> Vec<Part> {
    let tb = t.borrow();
    let mut parts: Vec<Part> = Vec::new();
    let mut buf = tb.text_or_empty();
    for child in &tb.children {
        let cb = child.borrow();
        if cb.is(HP, "lineBreak") {
            parts.push(Part::Text(std::mem::take(&mut buf)));
            parts.push(Part::Br);
            buf = cb.tail.clone().unwrap_or_default();
        } else {
            buf.push_str(&cb.text_or_empty());
            buf.push_str(cb.tail.as_deref().unwrap_or_default());
        }
    }
    parts.push(Part::Text(buf));
    parts
        .into_iter()
        .filter(|p| match p {
            Part::Br => true,
            Part::Text(s) => !s.is_empty(),
            Part::Table(_) => true,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 문단 수집 (본문·표 셀 공용)
// ---------------------------------------------------------------------------

/// 루트 아래에서 hp:tbl 내부로 내려가지 않고 hp:p를 문서 순서로 수집한다.
///
/// 본문(section 루트)과 표 셀(hp:tc) 양쪽에서 재사용한다. 표 안 문단은
/// 표 자체를 직렬화하는 쪽(`paragraph_parts`의 tbl 분기)이 처리하므로,
/// 여기서 tbl 내부까지 내려가면 같은 문단이 본문/셀 텍스트로 이중 출력된다.
pub fn collect_paragraphs(root: &NodeRef) -> Vec<NodeRef> {
    fn walk(node: &NodeRef, out: &mut Vec<NodeRef>) {
        let children = node.borrow().children.clone();
        for child in &children {
            let (is_p, is_tbl) = {
                let cb = child.borrow();
                (cb.is(HP, "p"), cb.is(HP, "tbl"))
            };
            if is_p {
                out.push(child.clone());
            }
            if !is_tbl {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

// ---------------------------------------------------------------------------
// 표: 직렬화와 배출 형식 결정
// ---------------------------------------------------------------------------

/// 표의 직계 행(hp:tr)만 반환한다.
///
/// 자손을 깊이 탐색하면 셀 안에 중첩된 표의 행까지 같은 목록에 섞여 바깥
/// 표가 깨진다 — 반드시 직계 자식만 취급한다.
pub fn table_rows(tbl: &NodeRef) -> Vec<NodeRef> {
    find_children(tbl, HP, "tr")
}

/// 셀 내부 문단을 마크다운으로 직렬화해 줄바꿈으로 연결한다.
///
/// 본문 문단과 동일한 파서(`paragraph_to_markdown`)를 재사용하므로 MEMO 제외,
/// hp:lineBreak 보존, 셀 안에 중첩된 표의 재귀 직렬화가 본문과 똑같이 적용된다.
/// Python 참조와 달리 **앞공백은 지우지 않는다**(뒷공백만 지운다) — md2hwpx
/// highlight 컴포넌트의 항목은 앞공백(빈 불릿 자리)이 렌더 서명이라, 이것이
/// 보존되어야 후처리가 highlight 표를 판정할 수 있다.
pub fn cell_text(tc: &NodeRef, stats: &mut ParseStats) -> String {
    collect_paragraphs(tc)
        .iter()
        .filter_map(|p| {
            let text = paragraph_to_markdown(p, stats);
            let trimmed_end = text.trim_end().to_string();
            if trimmed_end.trim().is_empty() {
                None
            } else {
                Some(trimmed_end)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// cellSpan 속성값을 정수로 읽는다. 빈 값·비정상 값은 1(병합 없음)로 본다.
fn parse_span(value: Option<String>) -> u32 {
    match value {
        Some(v) if !v.is_empty() => v.parse().unwrap_or(1),
        _ => 1,
    }
}

/// hp:tbl → colSpan/rowSpan 보존 HTML <table> (데이터 표 배출 형식).
pub fn table_to_html(tbl: &NodeRef, stats: &mut ParseStats) -> String {
    let mut html_rows: Vec<String> = Vec::new();
    for tr in table_rows(tbl) {
        let mut cells: Vec<String> = Vec::new();
        for tc in find_children(&tr, HP, "tc") {
            let mut span_attrs = String::new();
            if let Some(span) = find_child(&tc, HP, "cellSpan") {
                let sb = span.borrow();
                let col_span = parse_span(sb.get_attr("colSpan"));
                let row_span = parse_span(sb.get_attr("rowSpan"));
                if col_span > 1 {
                    span_attrs.push_str(&format!(" colspan=\"{col_span}\""));
                }
                if row_span > 1 {
                    span_attrs.push_str(&format!(" rowspan=\"{row_span}\""));
                }
            }
            let text = cell_text(&tc, stats).replace('\n', "<br>");
            cells.push(format!("<td{span_attrs}>{text}</td>"));
        }
        if !cells.is_empty() {
            html_rows.push(format!("<tr>{}</tr>", cells.join("")));
        }
    }
    if html_rows.is_empty() {
        return String::new();
    }
    format!("<table>{}</table>", html_rows.join(""))
}

/// 1행 표의 셀 목록. 1행 표가 아니면 None.
pub fn single_row_cells(tbl: &NodeRef) -> Option<Vec<NodeRef>> {
    if tbl.borrow().get_attr("rowCnt").as_deref() != Some("1") {
        return None;
    }
    let rows = table_rows(tbl);
    if rows.len() != 1 {
        return None;
    }
    Some(find_children(&rows[0], HP, "tc"))
}

/// 헤딩 후보 1행 표를 파이프 표(행+구분행)로 배출한다. 후보가 아니면 None.
///
/// 파이프 표로 배출된 후보는 공용 후처리(`postprocess_markdown`)가
/// title_bar_table_to_heading/subtitle_table_to_heading로 승격한다.
pub fn table_to_pipe_row(tbl: &NodeRef, stats: &mut ParseStats) -> Option<String> {
    let cells = single_row_cells(tbl)?;
    if cells.is_empty() {
        // 셀이 0개인 행(rowCnt="1"인데 hp:tr에 hp:tc가 없는 손상된 표)도 여기서
        // 걸러야 한다 — 그대로 title_bar_heading_from_cells에 넘기면 첫 셀
        // 인덱스 접근으로 변환 전체가 죽는다. Python 참조는 조용히 건너뛰지만
        // hwpx2md는 경고를 남긴다(출력이 완전하지 않다는 사실의 근거).
        stats
            .warnings
            .push("damaged table skipped: rowCnt=\"1\" row has no hp:tc cells".to_string());
        return None;
    }
    let raw_texts: Vec<String> = cells
        .iter()
        .map(|tc| cell_text(tc, stats))
        .collect();
    let texts: Vec<String> = raw_texts.iter().map(|t| t.trim().to_string()).collect();
    if texts.len() == 1 {
        // 셀 텍스트가 공백으로 시작하는 1×1 표는 md2hwpx highlight 컴포넌트의
        // 렌더 서명이다(항목 앞에 빈 불릿 자리가 공백으로 남는다). 소제목으로
        // 승격하지 않고 HTML로 나르면 후처리(box.rs)가 `===` 블록으로 복원한다.
        // Python 참조에는 이 판정이 없지만(하이라이트 개념 자체가 없다), 승격해
        // 버리면 왕복 시 하이라이트가 헤딩으로 변질된다.
        if raw_texts[0]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            return None;
        }
        if !texts[0].is_empty() && is_subtitle_text(&texts[0]) {
            // 셀이 2문단 이상이면 개행이 파이프 행 형식을 깬다 — `<br>`로
            // 인라인화한다(HTML 배출·파이프 표 경로와 같은 변환).
            return Some(format!("| {} |\n| --- |", texts[0].replace('\n', "<br>")));
        }
        return None;
    }
    if title_bar_heading_from_cells(&texts).is_none() {
        return None;
    }
    let row = format!("| {} |", texts.join(" | "));
    let sep = format!("| {} |", vec!["---"; texts.len()].join(" | "));
    Some(format!("{row}\n{sep}"))
}

/// 표를 배출 형식에 따라 직렬화한다.
///
/// 우선순위는 계약이다: 헤딩 후보 1행 파이프 → 병합 없는 데이터 표 파이프
/// 표 → colSpan/rowSpan 보존 HTML. 파이프 표는 md2hwpx 소스 파서(try_table)가
/// 그대로 다시 먹는 형식이라 왕복 손실이 없고, 병합 표만 HTML로 나른다
/// (md2hwpx에는 병합 표 문법이 없으므로 어차피 손실이다).
pub fn table_to_markdown(tbl: &NodeRef, stats: &mut ParseStats) -> String {
    if let Some(pipe) = table_to_pipe_row(tbl, stats) {
        stats.pipe_tables += 1;
        return pipe;
    }
    if let Some(pipe) = table_to_pipe_table(tbl, stats) {
        stats.pipe_tables += 1;
        return pipe;
    }
    let html = table_to_html(tbl, stats);
    if !html.is_empty() {
        stats.html_tables += 1;
    }
    html
}

/// 병합(colSpan/rowSpan > 1)이 없는 2열 이상 표를 파이프 표 전체(헤더 행 +
/// 구분 행 + 데이터 행)로 배출한다. 셀 안 개행은 `<br>`로 인라인화한다.
/// 후보가 아니면 None.
fn table_to_pipe_table(tbl: &NodeRef, stats: &mut ParseStats) -> Option<String> {
    let rows = table_rows(tbl);
    let mut lines: Vec<String> = Vec::new();
    for (i, tr) in rows.iter().enumerate() {
        let tcs = find_children(tr, HP, "tc");
        // 셀 안 개행(셀이 2문단 이상)은 `<br>`로 인라인화한다 — HTML 배출과
        // 같은 변환이고, md2hwpx 렌더가 `<br>`을 hp:lineBreak로 재흡수하므로
        // 왕복 시에도 줄바꿈이 보존된다. HTML로 나르면 재흡수 자체가 불가능하다.
        let cells: Vec<String> = tcs
            .iter()
            .map(|tc| cell_text(tc, stats).replace('\n', "<br>"))
            .collect();
        // 파이프 형식 제약: 2열 이상(md2hwpx try_table 요구), 셀 안 파이프 금지.
        if cells.len() < 2 || cells.iter().any(|c| c.contains('|')) {
            return None;
        }
        // 병합이 하나라도 있으면 파이프로 못 옮긴다.
        for tc in &tcs {
            if let Some(span) = find_child(&tc, HP, "cellSpan") {
                let sb = span.borrow();
                if parse_span(sb.get_attr("colSpan")) > 1 || parse_span(sb.get_attr("rowSpan")) > 1 {
                    return None;
                }
            }
        }
        lines.push(format!("| {} |", cells.join(" | ")));
        if i == 0 {
            lines.push(format!("| {} |", vec!["---"; cells.len()].join(" | ")));
        }
    }
    if lines.len() < 2 {
        return None;
    }
    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// 문단 → 마크다운 블록
// ---------------------------------------------------------------------------

/// 문단을 파트로 분해한다.
///
/// MEMO 필드 범위(fieldBegin → fieldEnd)는 형제 노드에 걸쳐 상태를 유지하며 제외한다.
/// MEMO 안에 다른 종류의 필드(하이퍼링크 등)가 중첩될 수 있으므로, fieldBegin의
/// id와 fieldEnd의 beginIDRef로 짝을 맞춘다(HWPX 스키마: fieldEnd는 자신을
/// 연 fieldBegin을 beginIDRef로 참조하며, "id"라는 자기 속성이 아니다) —
/// 아무 fieldEnd나 만나면 닫는다고 하면 중첩 필드의 fieldEnd가 바깥 MEMO
/// 범위를 조기 종료시켜 그 뒤 메모 내용이 본문으로 새어나간다.
/// 표(hp:tbl)를 만나면 그 자리에서 재귀적으로 직렬화하므로, 셀 안에 또 다른
/// 표가 있어도 자연스럽게 중첩된다.
pub fn paragraph_parts(p: &NodeRef, stats: &mut ParseStats) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    let mut memo_id: Option<String> = None;
    let children = p.borrow().children.clone();
    for child in &children {
        walk_parts(child, &mut parts, &mut memo_id, stats);
    }
    parts
}

fn walk_parts(
    node: &NodeRef,
    parts: &mut Vec<Part>,
    memo_id: &mut Option<String>,
    stats: &mut ParseStats,
) {
    let (is_memo_begin, begin_id, is_field_end, begin_id_ref, is_linebreak, is_tbl, is_t) = {
        let b = node.borrow();
        (
            b.is(HP, "fieldBegin") && b.get_attr("type").as_deref() == Some("MEMO"),
            b.get_attr("id"),
            b.is(HP, "fieldEnd"),
            b.get_attr("beginIDRef"),
            b.is(HP, "lineBreak"),
            b.is(HP, "tbl"),
            b.is(HP, "t"),
        )
    };
    if is_memo_begin && memo_id.is_none() {
        *memo_id = begin_id;
        return;
    }
    if is_field_end {
        if memo_id.is_some() && begin_id_ref.as_deref() == memo_id.as_deref() {
            *memo_id = None;
        }
        return;
    }
    if memo_id.is_some() {
        // MEMO 범위 안의 텍스트/블록은 건너뛰되, 매칭되는 fieldEnd를
        // 감시하려면(중첩 필드 포함) 계속 내려가야 한다
        let children = node.borrow().children.clone();
        for child in &children {
            walk_parts(child, parts, memo_id, stats);
        }
        return;
    }
    if is_linebreak {
        parts.push(Part::Br);
        return;
    }
    if is_tbl {
        let markdown = table_to_markdown(node, stats);
        if !markdown.is_empty() {
            parts.push(Part::Table(markdown));
        }
        return;
    }
    if is_t {
        parts.extend(t_parts(node));
        return;
    }
    let children = node.borrow().children.clone();
    for child in &children {
        walk_parts(child, parts, memo_id, stats);
    }
}

pub fn paragraph_to_markdown(p: &NodeRef, stats: &mut ParseStats) -> String {
    let parts = paragraph_parts(p, stats);
    if parts.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for part in &parts {
        match part {
            Part::Table(content) => {
                out.push('\n');
                out.push_str(content);
                out.push('\n');
            }
            Part::Br => out.push('\n'),
            Part::Text(content) => out.push_str(content),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 진입점
// ---------------------------------------------------------------------------

/// 모든 section을 번호 순서대로 문단 단위 마크다운 블록으로 바꾼다.
/// 블록 사이는 lib의 파이프라인이 빈 줄 두 개로 연결하고, 공용 후처리가
/// 이어서 문서 관습을 복원한다.
pub fn sections_to_blocks(section_roots: &[NodeRef]) -> (Vec<String>, ParseStats) {
    let mut blocks: Vec<String> = Vec::new();
    let mut stats = ParseStats::default();
    for root in section_roots {
        for p in collect_paragraphs(root) {
            let text = paragraph_to_markdown(&p, &mut stats);
            if !text.trim().is_empty() {
                blocks.push(text);
            }
        }
    }
    (blocks, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::xmltree::{find_desc, parse};

    fn root_of(inner: &str) -> NodeRef {
        let doc = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="{HP}">{inner}</hp:hs>"#
        );
        parse(doc.as_bytes()).unwrap()
    }

    #[test]
    fn t_parts_splits_at_nested_line_break() {
        // 줄바꿈이 run 경계가 아니라 t 내부에 중첩된 실측 사례.
        let root = root_of("<hp:t>앞<hp:lineBreak/>뒤</hp:t>");
        let t = find_desc(&root, HP, "t").unwrap();
        assert_eq!(
            t_parts(&t),
            vec![
                Part::Text("앞".to_string()),
                Part::Br,
                Part::Text("뒤".to_string())
            ]
        );
    }

    #[test]
    fn t_parts_recovers_tail_only_text() {
        // 글머리표·들여쓰기가 자식 요소의 tail에만 존재하는 실측 사례.
        let root = root_of("<hp:t><hp:autoNum/>● 항목 텍스트</hp:t>");
        let t = find_desc(&root, HP, "t").unwrap();
        assert_eq!(t_parts(&t), vec![Part::Text("● 항목 텍스트".to_string())]);
    }

    #[test]
    fn t_parts_sees_fwspace_absorbed_text_end_to_end() {
        // fwSpace 흡수가 t_parts까지 이어지는지(xmltree 계층만이 아니라) 확인.
        let root = root_of("<hp:t>’26.<hp:fwSpace/>8.<hp:fwSpace/>25(금)</hp:t>");
        let t = find_desc(&root, HP, "t").unwrap();
        assert_eq!(
            t_parts(&t),
            vec![Part::Text("’26. 8. 25(금)".to_string())]
        );
    }

    #[test]
    fn memo_exclusion_matches_begin_id_ref_not_any_field_end() {
        // MEMO 안에 하이퍼링크 필드가 중첩된 실측 사례. 아무 fieldEnd나 만나면
        // 닫는다고 하면 h1의 fieldEnd가 MEMO 범위를 조기 종료시켜 메모 내용이
        // 본문으로 새어나간다.
        let root = root_of(
            "<hp:p>\
             <hp:run><hp:fieldBegin type=\"MEMO\" id=\"m1\"/></hp:run>\
             <hp:run><hp:fieldBegin type=\"HYPERLINK\" id=\"h1\"/><hp:t>링크</hp:t><hp:fieldEnd beginIDRef=\"h1\"/></hp:run>\
             <hp:run><hp:t>메모 본문</hp:t></hp:run>\
             <hp:run><hp:fieldEnd beginIDRef=\"m1\"/></hp:run>\
             <hp:run><hp:t>바깥 텍스트</hp:t></hp:run>\
             </hp:p>",
        );
        let p = find_desc(&root, HP, "p").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(paragraph_to_markdown(&p, &mut stats), "바깥 텍스트");
    }

    #[test]
    fn hyperlink_field_text_outside_memo_is_kept() {
        let root = root_of(
            "<hp:p>\
             <hp:run><hp:fieldBegin type=\"HYPERLINK\" id=\"h1\"/><hp:t>링크</hp:t><hp:fieldEnd beginIDRef=\"h1\"/></hp:run>\
             </hp:p>",
        );
        let p = find_desc(&root, HP, "p").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(paragraph_to_markdown(&p, &mut stats), "링크");
    }

    #[test]
    fn collect_paragraphs_never_descends_into_tbl() {
        // 표 안 문단이 본문으로 이중 출력되지 않게 한다.
        let root = root_of(
            "<hp:p><hp:run><hp:t>앞문단</hp:t></hp:run></hp:p>\
             <hp:tbl><hp:tr><hp:tc><hp:p><hp:run><hp:t>셀 문단</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>\
             <hp:p><hp:run><hp:t>뒷문단</hp:t></hp:run></hp:p>",
        );
        let paragraphs = collect_paragraphs(&root);
        assert_eq!(paragraphs.len(), 2);
    }

    #[test]
    fn damaged_single_row_table_does_not_panic_and_warns() {
        // rowCnt="1"인데 hp:tc가 없는 손상된 표 — 인덱스 접근하면 죽는다.
        let root = root_of(
            "<hp:p><hp:tbl rowCnt=\"1\"><hp:tr></hp:tr></hp:tbl></hp:p>",
        );
        let p = find_desc(&root, HP, "p").unwrap();
        let mut stats = ParseStats::default();
        let md = paragraph_to_markdown(&p, &mut stats);
        assert_eq!(md, "");
        assert_eq!(stats.warnings.len(), 1);
        assert!(stats.warnings[0].contains("no hp:tc cells"));
    }

    #[test]
    fn table_to_html_preserves_col_and_row_span() {
        let root = root_of(
            "<hp:tbl rowCnt=\"1\">\
             <hp:tr>\
             <hp:tc><hp:cellSpan colSpan=\"2\"/><hp:p><hp:run><hp:t>병합</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:cellSpan rowSpan=\"2\"/><hp:p><hp:run><hp:t>세로</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr>\
             </hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_html(&tbl, &mut stats),
            "<table><tr><td colspan=\"2\">병합</td><td rowspan=\"2\">세로</td></tr></table>"
        );
    }

    #[test]
    fn cell_newlines_become_br_in_html() {
        let root = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>첫줄</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>둘째줄</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_html(&tbl, &mut stats),
            "<table><tr><td>첫줄<br>둘째줄</td></tr></table>"
        );
    }

    #[test]
    fn subtitle_table_emits_pipe_row() {
        let root = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr><hp:tc><hp:p><hp:run><hp:t>2.1 개요</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "| 2.1 개요 |\n| --- |"
        );
        assert_eq!(stats.pipe_tables, 1);
        assert_eq!(stats.html_tables, 0);
    }

    #[test]
    fn title_bar_table_emits_pipe_row() {
        let bar = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>5</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>현지화 추진현황</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&bar, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "| 5 | 현지화 추진현황 |\n| --- | --- |"
        );
        assert_eq!(stats.pipe_tables, 1);
        assert_eq!(stats.html_tables, 0);
    }

    #[test]
    fn spanless_data_table_emits_full_pipe_table() {
        // 병합 없는 데이터 표는 md2hwpx가 다시 먹는 파이프 표 전체로 나른다.
        let data = root_of(
            "<hp:tbl rowCnt=\"2\"><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>구분</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>결과</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>빌드</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>성공</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&data, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "| 구분 | 결과 |\n| --- | --- |\n| 빌드 | 성공 |"
        );
        assert_eq!(stats.pipe_tables, 1);
        assert_eq!(stats.html_tables, 0);
    }

    #[test]
    fn multiline_cells_in_spanless_table_become_br_in_pipe_table() {
        // 셀이 2문단 이상인 병합 없는 표도 파이프 표로 나른다. 셀 안 개행은
        // `<br>`로 인라인화하고(md2hwpx 렌더가 hp:lineBreak로 재흡수), HTML로
        // 떨어뜨리면 md2hwpx 재흡수가 아예 불가능하므로 왕복 정합성에 반한다.
        let root = root_of(
            "<hp:tbl rowCnt=\"2\"><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>요구사항 해석</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>역무 범위 검토</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>EPC Scope</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>빌드</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>성공</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "| 요구사항 해석<br>역무 범위 검토 | EPC Scope |\n| --- | --- |\n| 빌드 | 성공 |"
        );
        assert_eq!(stats.pipe_tables, 1);
        assert_eq!(stats.html_tables, 0);
    }

    #[test]
    fn multiline_subtitle_cell_becomes_br_in_pipe_row() {
        // 1×1 소제목 표의 셀이 2문단 이상이면 개행이 파이프 행 형식을 깬다 —
        // `<br>`로 인라인화한다.
        let root = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr><hp:tc>\
             <hp:p><hp:run><hp:t>2.1 개요</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>배경</hp:t></hp:run></hp:p>\
             </hp:tc></hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "| 2.1 개요<br>배경 |\n| --- |"
        );
        assert_eq!(stats.pipe_tables, 1);
        assert_eq!(stats.html_tables, 0);
    }

    #[test]
    fn merged_table_falls_back_to_html() {
        let root = root_of(
            "<hp:tbl rowCnt=\"1\">\
             <hp:tr>\
             <hp:tc><hp:cellSpan colSpan=\"2\"/><hp:p><hp:run><hp:t>병합</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:cellSpan rowSpan=\"2\"/><hp:p><hp:run><hp:t>세로</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr>\
             </hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "<table><tr><td colspan=\"2\">병합</td><td rowspan=\"2\">세로</td></tr></table>"
        );
        assert_eq!(stats.html_tables, 1);
    }

    #[test]
    fn single_column_and_pipe_containing_tables_fall_back_to_html() {
        // 1열 표는 md2hwpx 표 흡수(2열 이상)와 맞지 않고, 셀의 파이프는
        // 파이프 형식을 깨뜨린다 — 둘 다 HTML로 나른다.
        let single = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr><hp:tc><hp:p><hp:run><hp:t>끝났다.</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&single, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "<table><tr><td>끝났다.</td></tr></table>"
        );

        let piped = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr>\
             <hp:tc><hp:p><hp:run><hp:t>a|b</hp:t></hp:run></hp:p></hp:tc>\
             <hp:tc><hp:p><hp:run><hp:t>c</hp:t></hp:run></hp:p></hp:tc>\
             </hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&piped, HP, "tbl").unwrap();
        assert!(table_to_markdown(&tbl, &mut stats).starts_with("<table>"));
    }

    #[test]
    fn leading_whitespace_single_cell_is_not_a_subtitle_candidate() {
        // md2hwpx highlight 컴포넌트의 렌더 서명(셀 텍스트가 공백으로 시작) —
        // 소제목으로 승격되지 않고 HTML로 나르면 후처리가 === 블록으로 복원한다.
        let root = root_of(
            "<hp:tbl rowCnt=\"1\"><hp:tr><hp:tc><hp:p><hp:run><hp:t> 본 문서는 예시 문서임</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>",
        );
        let tbl = find_desc(&root, HP, "tbl").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            table_to_markdown(&tbl, &mut stats),
            "<table><tr><td> 본 문서는 예시 문서임</td></tr></table>"
        );
        assert_eq!(stats.pipe_tables, 0);
    }

    #[test]
    fn paragraph_serializes_table_in_place() {
        // 본문 문단 중간의 표는 그 자리에서 직렬화된다.
        let root = root_of(
            "<hp:p><hp:run><hp:t>앞</hp:t></hp:run>\
             <hp:tbl rowCnt=\"1\"><hp:tr><hp:tc><hp:p><hp:run><hp:t>2.1 개요</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>\
             </hp:p>",
        );
        let p = find_desc(&root, HP, "p").unwrap();
        let mut stats = ParseStats::default();
        assert_eq!(
            paragraph_to_markdown(&p, &mut stats),
            "앞\n| 2.1 개요 |\n| --- |\n"
        );
    }

    #[test]
    fn sections_to_blocks_collects_nonblank_paragraphs() {
        let a = root_of(
            "<hp:p><hp:run><hp:t>첫 문단</hp:t></hp:run></hp:p><hp:p></hp:p>",
        );
        let b = root_of("<hp:p><hp:run><hp:t>둘 문단</hp:t></hp:run></hp:p>");
        let (blocks, stats) = sections_to_blocks(&[a, b]);
        assert_eq!(blocks, vec!["첫 문단".to_string(), "둘 문단".to_string()]);
        assert!(stats.warnings.is_empty());
    }
}
