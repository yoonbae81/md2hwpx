use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::shared::error::{AppError, Result};
use crate::compile::patterns::{MARKDOWN_BOLD_RE, PARENTHESIS_RE};
use crate::compile::render::replace_run_text;
use crate::shared::xmltree::{
    append_child, deep_clone, element_index, find_child, find_children, find_desc,
    insert_element, iter_elements, remove_child, serialize_signature, set_attr, Element, HH, HP,
    NodeRef,
};

fn new_element(prefix: &str, local: &str, ns: &str) -> NodeRef {
    Rc::new(RefCell::new(Element::new(prefix, local, ns)))
}

/// id 속성을 제외한 charPr 직렬화 시그니처.
fn char_pr_signature(char_pr: &NodeRef) -> Vec<u8> {
    let clone = deep_clone(char_pr);
    clone.borrow_mut().remove_attr("id");
    serialize_signature(&clone)
}

fn sorted_numeric(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_by_key(|id| id.parse::<i64>().unwrap_or(i64::MAX));
    ids.dedup();
    ids
}

/// base charPr를 복제해 `derive`로 변형한 뒤, 시그니처가 같은 기존
/// charPr을 재사용하거나 새 ID를 할당해 charProperties에 추가한다.
/// 모든 후처리가 이 ID 할당 경로를 공유한다.
fn derived_char_pr_ids(
    header_root: &NodeRef,
    base_ids: &[String],
    derive: impl Fn(&NodeRef),
) -> Result<HashMap<String, String>> {
    let char_properties = find_desc(header_root, HH, "charProperties")
        .ok_or_else(|| AppError("header.xml has no charProperties.".into()))?;
    let char_prs = find_children(&char_properties, HH, "charPr");
    let mut by_signature: HashMap<Vec<u8>, String> = HashMap::new();
    for cp in &char_prs {
        by_signature.insert(char_pr_signature(cp), cp.borrow().get_attr("id").unwrap_or_default());
    }
    let mut next_id: i64 = char_prs
        .iter()
        .map(|cp| {
            cp.borrow()
                .get_attr("id")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(-1)
        })
        .max()
        .unwrap_or(-1)
        + 1;

    let mut derived_ids = HashMap::new();
    for base_id in sorted_numeric(base_ids.to_vec()) {
        let base = char_prs
            .iter()
            .find(|cp| cp.borrow().get_attr("id").as_deref() == Some(base_id.as_str()))
            .ok_or_else(|| {
                AppError(format!("charPrIDRef={base_id} not found in header.xml."))
            })?;
        let variant = deep_clone(base);
        set_attr(&variant, "id", &next_id.to_string());
        derive(&variant);
        let signature = char_pr_signature(&variant);
        let derived_id = match by_signature.get(&signature) {
            Some(existing) => existing.clone(),
            None => {
                let id = next_id.to_string();
                next_id += 1;
                append_child(&char_properties, variant.clone());
                by_signature.insert(signature, id.clone());
                id
            }
        };
        derived_ids.insert(base_id.clone(), derived_id);
    }
    let count = find_children(&char_properties, HH, "charPr").len();
    set_attr(&char_properties, "itemCnt", &count.to_string());
    Ok(derived_ids)
}

/// 각 기본 스타일을 유지하면서 Bold만 추가한 charPr ID를 만든다.
fn bold_char_pr_ids(
    header_root: &NodeRef,
    base_ids: &[String],
) -> Result<HashMap<String, String>> {
    derived_char_pr_ids(header_root, base_ids, |variant| {
        if find_child(variant, HH, "bold").is_some() {
            return;
        }
        let insert_at = find_child(variant, HH, "offset")
            .and_then(|off| element_index(variant, &off))
            .map(|i| i + 1)
            .unwrap_or(0);
        insert_element(variant, insert_at, new_element("hh", "bold", HH));
    })
}

/// 각 기본 스타일을 유지하면서 붉은 윗첨자만 추가한 charPr ID를 만든다.
fn superscript_red_char_pr_ids(
    header_root: &NodeRef,
    base_ids: &[String],
) -> Result<HashMap<String, String>> {
    derived_char_pr_ids(header_root, base_ids, |variant| {
        set_attr(variant, "textColor", "#FF0000");
        if find_child(variant, HH, "supscript").is_none() {
            append_child(variant, new_element("hh", "supscript", HH));
        }
    })
}

/// 문자 높이를 HWP 단위만큼 조정한 charPr ID를 만든다.
fn scaled_char_pr_ids(
    header_root: &NodeRef,
    base_ids: &[String],
    delta: i64,
) -> Result<HashMap<String, String>> {
    derived_char_pr_ids(header_root, base_ids, move |variant| {
        let height: i64 = variant
            .borrow()
            .get_attr("height")
            .and_then(|h| h.parse::<i64>().ok())
            .unwrap_or(1);
        set_attr(variant, "height", &std::cmp::max(1, height + delta).to_string());
    })
}

/// run 분할 조각. LineBreak는 원본 자식을 그대로 옮기고, Text는 charPr를
/// 붙여 새 run으로 나온다(None이면 run의 원래 속성).
enum Segment {
    LineBreak(NodeRef),
    Text(String, Option<String>),
}

/// run을 부모에서 떼어 내고 조각 순서대로 복제 run을 끼워 넣는다.
/// 모든 후처리가 같은 배치 코드를 공유하며, 부모는 run을 발견한 순회에서
/// 얻어 온다(트리를 다시 뒤지지 않는다).
fn split_run(parent: &NodeRef, run: &NodeRef, segments: Vec<Segment>) -> Result<()> {
    let start = element_index(parent, run)
        .ok_or_else(|| AppError("run has no parent paragraph.".into()))?;
    remove_child(parent, run);
    for (offset, segment) in segments.into_iter().enumerate() {
        let clone = deep_clone(run);
        match segment {
            Segment::LineBreak(child) => {
                clone.borrow_mut().children.clear();
                append_child(&clone, child);
            }
            Segment::Text(text, char_pr) => {
                replace_run_text(&clone, &text, char_pr.as_deref());
            }
        }
        insert_element(parent, start + offset, clone);
    }
    Ok(())
}

/// 대상 run들의 charPrIDRef를 중복 없이 모은다.
fn unique_char_pr_refs<'a>(runs: impl Iterator<Item = &'a NodeRef>) -> Vec<String> {
    let mut seen = HashSet::new();
    runs.filter_map(|r| r.borrow().get_attr("charPrIDRef"))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// run 재구성 후처리 한 가지의 규칙. 텍스트 노드별로 파생 charPr을 먹일
/// 구간을 찾고, 그 구간에 쓸 charPr 변형을 header에 등록하는 방법만 정의하면
/// 스캔·ID 할당·run 분할은 `apply_run_restyler` 엔진이 한다. 새 후처리는
/// 이 트레이트를 구현하고 hwpx.rs의 파이프라인에 한 줄을 추가하는 것으로 끝난다.
trait RunRestyler {
    /// run을 검사 대상으로 볼지. false면 텍스트를 검사하지 않고 선행 문자
    /// 컨텍스트도 갱신하지 않는다.
    fn scans_run(&self, run: &NodeRef) -> bool {
        let _ = run;
        true
    }

    /// 텍스트가 아닌 컨트롤이 섞인 run의 텍스트도 검사할지. true면 검사한
    /// 뒤 `restyle_mixed_run`으로 통째로 처리한다.
    fn scans_mixed_runs(&self) -> bool {
        false
    }

    /// 텍스트가 아닌 컨트롤이 섞인 run인지. lineBreak는 텍스트 흐름의
    /// 일부라 섞인 것으로 보지 않는다.
    fn is_mixed_run(&self, run: &NodeRef) -> bool {
        run.borrow().children.iter().any(|c| {
            let b = c.borrow();
            !(b.is(HP, "t") || b.is(HP, "lineBreak"))
        })
    }

    /// 한 텍스트 노드를 (조각 텍스트, 파생 charPr 구간인지) 순서대로
    /// 쪼갠다. `preceding`은 같은 문단에서 바로 앞 텍스트의 마지막 문자
    /// (줄 시작/줄바꿈/컨트롤 직후는 None)다. 조각을 이어 붙이면 원본
    /// 텍스트와 같아야 한다(굵게처럼 표식을 지우는 규칙도 포함).
    fn split_node(&self, text: &str, preceding: Option<char>) -> Vec<(String, bool)>;

    /// base charPr ID별 파생 charPr ID를 header에 만든다(같은 시그니처 재사용).
    fn derive_ids(
        &self,
        header_root: &NodeRef,
        base_ids: &[String],
    ) -> Result<HashMap<String, String>>;

    /// 컨트롤이 섞인 run의 대체 처리. run을 분할하지 않고 통째로 처리했으면
    /// true. 기본은 분할 경로를 쓰라는 뜻의 false다.
    fn restyle_mixed_run(
        &self,
        run: &NodeRef,
        derived: &HashMap<String, String>,
    ) -> Result<bool> {
        let _ = (run, derived);
        Ok(false)
    }
}

/// 한 규칙이 짚은 run: (부모, run, 텍스트 노드별 조각 분할).
struct RestyleMatches {
    parent: NodeRef,
    run: NodeRef,
    nodes: Vec<(NodeRef, Vec<(String, bool)>)>,
}

/// 문단의 run 텍스트를 줄 순서대로 훑으며 규칙별 파생 구간을 수집한다.
/// 선행 문자는 같은 문단 안에서 텍스트 노드 사이를 이어 주고, 줄바꿈과
/// 컨트롤에서 끊는다.
fn scan_matches(rule: &dyn RunRestyler, section_root: &NodeRef) -> Vec<RestyleMatches> {
    let mut out = Vec::new();
    for container in iter_elements(section_root) {
        let mut previous: Option<char> = None;
        for run in container.borrow().children.clone() {
            if !run.borrow().is(HP, "run") {
                continue;
            }
            if !rule.scans_run(&run) {
                continue;
            }
            if rule.is_mixed_run(&run) && !rule.scans_mixed_runs() {
                // 컨트롤이 섞인 run은 검사하지 않고 컨텍스트만 끊는다.
                previous = None;
                continue;
            }
            let mut nodes = Vec::new();
            for child in run.borrow().children.clone() {
                if child.borrow().is(HP, "lineBreak") {
                    previous = None;
                    continue;
                }
                if !child.borrow().is(HP, "t") {
                    continue;
                }
                let text = child.borrow().text_or_empty();
                let pieces = rule.split_node(&text, previous);
                if pieces.iter().any(|(_, derived)| *derived) {
                    nodes.push((child.clone(), pieces));
                }
                if !text.is_empty() {
                    previous = text.chars().last();
                }
            }
            if !nodes.is_empty() {
                out.push(RestyleMatches {
                    parent: container.clone(),
                    run,
                    nodes,
                });
            }
        }
    }
    out
}

/// 정렬된 파생 구간을 [바깥=base, 안=파생] 조각으로 자르는 공용 헬퍼.
/// 표식을 지우지 않는 규칙(괄호, 별표)이 쓴다.
fn spans_to_pieces(text: &str, spans: &[(usize, usize)]) -> Vec<(String, bool)> {
    if spans.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let mut pieces = Vec::new();
    let mut cursor = 0usize;
    for (start, end) in spans {
        if *start > cursor {
            pieces.push((text[cursor..*start].to_string(), false));
        }
        pieces.push((text[*start..*end].to_string(), true));
        cursor = *end;
    }
    if cursor < text.len() {
        pieces.push((text[cursor..].to_string(), false));
    }
    pieces
}

/// 후처리 엔진: 스캔 → charPr 변형 ID 등록 → run 재구성. 굵게/괄호/붙은
/// 별표가 같은 경로를 쓰며, 매치가 하나라도 있으면 true를 돌려준다.
fn apply_run_restyler(
    rule: &dyn RunRestyler,
    section_root: &NodeRef,
    header_root: &NodeRef,
) -> Result<bool> {
    let matches = scan_matches(rule, section_root);
    if matches.is_empty() {
        return Ok(false);
    }
    let base_ids = unique_char_pr_refs(matches.iter().map(|m| &m.run));
    let derived = rule.derive_ids(header_root, &base_ids)?;
    for m in &matches {
        let base_id = m.run.borrow().get_attr("charPrIDRef");
        let derived_id = base_id.as_ref().and_then(|b| derived.get(b).cloned());
        if rule.is_mixed_run(&m.run) {
            rule.restyle_mixed_run(&m.run, &derived)?;
            continue;
        }
        let mut segments = Vec::new();
        for child in m.run.borrow().children.clone() {
            if child.borrow().is(HP, "lineBreak") {
                segments.push(Segment::LineBreak(deep_clone(&child)));
                continue;
            }
            if !child.borrow().is(HP, "t") {
                continue;
            }
            let text = child.borrow().text_or_empty();
            let pieces = m
                .nodes
                .iter()
                .find(|(c, _)| Rc::ptr_eq(c, &child))
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| vec![(text, false)]);
            for (part, derived_span) in pieces {
                let char_pr = if derived_span {
                    derived_id.clone()
                } else {
                    base_id.clone()
                };
                segments.push(Segment::Text(part, char_pr));
            }
        }
        split_run(&m.parent, &m.run, segments)?;
    }
    Ok(true)
}

/// `**굵게**` 표식을 제거하고 안쪽 텍스트에 Bold charPr을 배정한다.
/// 마크다운 굵게는 노드 안에서만 짝이 맞는다.
struct MarkdownBold;

impl RunRestyler for MarkdownBold {
    // 줄바꿈이 섞인 run도 예전 동작대로 컨트롤 run처럼 통째로 처리한다.
    fn scans_mixed_runs(&self) -> bool {
        true
    }

    fn is_mixed_run(&self, run: &NodeRef) -> bool {
        run.borrow().children.iter().any(|c| !c.borrow().is(HP, "t"))
    }

    fn split_node(&self, text: &str, _preceding: Option<char>) -> Vec<(String, bool)> {
        // 굵은 구간은 표식을 제거한 안쪽 텍스트가 되고, 그 밖은 원래
        // charPr을 유지한다.
        let mut pieces = Vec::new();
        let mut last = 0usize;
        for c in MARKDOWN_BOLD_RE.captures_iter(text) {
            let m = c.get(0).unwrap();
            if m.start() > last {
                pieces.push((text[last..m.start()].to_string(), false));
            }
            pieces.push((c[1].to_string(), true));
            last = m.end();
        }
        if last < text.len() {
            pieces.push((text[last..].to_string(), false));
        }
        pieces
    }

    fn derive_ids(
        &self,
        header_root: &NodeRef,
        base_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        bold_char_pr_ids(header_root, base_ids)
    }

    fn restyle_mixed_run(
        &self,
        run: &NodeRef,
        derived: &HashMap<String, String>,
    ) -> Result<bool> {
        let base_id = run.borrow().get_attr("charPrIDRef");
        let bold_id = base_id.as_ref().and_then(|b| derived.get(b).cloned());
        // 컨트롤이 있는 run은 그대로 두고 run 전체를 굵게, 표식만 제거.
        if let Some(bid) = &bold_id {
            set_attr(run, "charPrIDRef", bid);
        }
        for node in run.borrow().children.clone() {
            let marked = node.borrow().is(HP, "t")
                && MARKDOWN_BOLD_RE.is_match(node.borrow().text_or_empty().as_str());
            if !marked {
                continue;
            }
            let mut nb = node.borrow_mut();
            if let Some(t) = &nb.text {
                nb.text = Some(t.replace("**", ""));
            }
        }
        Ok(true)
    }
}

/// 붙은 괄호 텍스트(공백 없이 앞 글자와 붙은 경우)를 `delta`만큼 줄인다.
struct AttachedParenthesis {
    delta: i64,
}

impl RunRestyler for AttachedParenthesis {
    fn split_node(&self, text: &str, preceding: Option<char>) -> Vec<(String, bool)> {
        spans_to_pieces(text, &self.attached_spans(text, preceding))
    }

    fn derive_ids(
        &self,
        header_root: &NodeRef,
        base_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        scaled_char_pr_ids(header_root, base_ids, self.delta)
    }
}

impl AttachedParenthesis {
    /// 붙은 괄호의 바이트 구간. None은 줄 시작/줄바꿈 직후를 뜻한다
    /// (공백 아님 → 붙은 것으로 간주).
    fn attached_spans(&self, text: &str, preceding: Option<char>) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        for m in PARENTHESIS_RE.find_iter(text) {
            let before = if m.start() > 0 {
                text[..m.start()].chars().last()
            } else {
                preceding
            };
            if !matches!(before, Some(' ') | Some('\t')) {
                spans.push((m.start(), m.end()));
            }
        }
        spans
    }
}

/// 글자 뒤에 붙어 나오는 `*`(앞이 공백이 아닌 별표)를 윗첨자 빨간색으로
/// 배정한다. red.hwpx 참조 스타일: textColor=#FF0000 + hh:supscript.
struct AttachedAsterisk;

impl RunRestyler for AttachedAsterisk {
    fn scans_run(&self, run: &NodeRef) -> bool {
        run.borrow().get_attr("charPrIDRef").is_some()
    }

    fn split_node(&self, text: &str, preceding: Option<char>) -> Vec<(String, bool)> {
        spans_to_pieces(text, &self.attached_spans(text, preceding))
    }

    fn derive_ids(
        &self,
        header_root: &NodeRef,
        base_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        superscript_red_char_pr_ids(header_root, base_ids)
    }
}

impl AttachedAsterisk {
    /// 붙은 별표의 바이트 구간('*'는 1바이트).
    fn attached_spans(&self, text: &str, preceding: Option<char>) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        for (index, c) in text.char_indices() {
            if c != '*' {
                continue;
            }
            let before = if index == 0 {
                preceding
            } else {
                text[..index].chars().last()
            };
            // '*' 바로 앞의 '*'는 굵게 표식 잔여로 보고 붙은 것으로
            // 치지 않는다.
            if before.is_some_and(|pc| !pc.is_whitespace() && pc != '*') {
                spans.push((index, index + 1));
            }
        }
        spans
    }
}

/// Markdown 굵게 표식을 제거하고 대응 Bold charPr 스타일을 배정한다.
pub fn apply_markdown_bold(section_root: &NodeRef, header_root: &NodeRef) -> Result<bool> {
    apply_run_restyler(&MarkdownBold, section_root, header_root)
}

/// 붙은 괄호 텍스트(공백 없이 앞 글자와 붙은 경우)를 2pt 줄인다.
pub fn apply_parenthesis_size(section_root: &NodeRef, header_root: &NodeRef) -> Result<bool> {
    apply_run_restyler(
        &AttachedParenthesis { delta: -200 },
        section_root,
        header_root,
    )
}

/// 글자 뒤에 붙어 나오는 `*`를 윗첨자 빨간색으로 배정한다. 굵게/괄호
/// 후처리가 run을 재구성하므로 반드시 그 뒤에 실행해야 한다.
pub fn apply_attached_asterisk(section_root: &NodeRef, header_root: &NodeRef) -> Result<bool> {
    apply_run_restyler(&AttachedAsterisk, section_root, header_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::xmltree::{find_all_desc, parse};

    fn run_texts(root: &NodeRef) -> Vec<String> {
        find_all_desc(root, HP, "run")
            .iter()
            .map(|r| {
                find_children(r, HP, "t")
                    .iter()
                    .map(|t| t.borrow().text_or_empty())
                    .collect::<String>()
            })
            .collect()
    }

    fn run_text_of(run: &NodeRef) -> String {
        find_children(run, HP, "t")
            .iter()
            .map(|t| t.borrow().text_or_empty())
            .collect()
    }

    fn char_pr_heights(header: &NodeRef) -> Vec<(String, Option<String>)> {
        find_desc(header, HH, "charProperties")
            .map(|props| {
                find_children(&props, HH, "charPr")
                    .iter()
                    .map(|cp| {
                        (
                            cp.borrow().get_attr("id").unwrap_or_default(),
                            cp.borrow().get_attr("height"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn markdown_bold_splits_runs_and_derives_bold_char_pr() {
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>앞 **굵게** 뒤</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="1"><hh:charPr id="7" height="1000"/></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_markdown_bold(&section, &header).unwrap());
        assert_eq!(run_texts(&section), vec!["앞 ", "굵게", " 뒤"]);
        let props = find_desc(&header, HH, "charProperties").unwrap();
        let prs = find_children(&props, HH, "charPr");
        assert_eq!(prs.len(), 2);
        assert!(find_child(&prs[1], HH, "bold").is_some());
        let bold_run = find_all_desc(&section, HP, "run")
            .into_iter()
            .find(|r| run_text_of(r) == "굵게")
            .unwrap();
        assert_eq!(bold_run.borrow().get_attr("charPrIDRef").as_deref(), Some("8"));
    }

    #[test]
    fn attached_parenthesis_shrinks_only_attached() {
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>제안(안) (별지)</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="1"><hh:charPr id="7" height="1000"/></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_parenthesis_size(&section, &header).unwrap());
        assert_eq!(run_texts(&section), vec!["제안", "(안)", " (별지)"]);
        // "(안)"은 붙었으므로 2pt 줄인 charPr, "(별지)"는 원래 charPr이다.
        let heights = char_pr_heights(&header);
        assert_eq!(heights.len(), 2);
        assert_eq!(heights[1].1.as_deref(), Some("800"));
        let runs = find_all_desc(&section, HP, "run");
        let shrunk = runs
            .iter()
            .find(|r| run_text_of(r) == "(안)")
            .unwrap()
            .borrow()
            .get_attr("charPrIDRef")
            .unwrap();
        assert_eq!(shrunk, heights[1].0);
        let kept = runs
            .iter()
            .find(|r| run_text_of(r) == " (별지)")
            .unwrap()
            .borrow()
            .get_attr("charPrIDRef");
        assert_eq!(kept.as_deref(), Some("7"));
    }

    #[test]
    fn attached_asterisk_becomes_red_superscript() {
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>제안* 필요</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="1"><hh:charPr id="7" height="1000" textColor="#000000"/></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_attached_asterisk(&section, &header).unwrap());
        assert_eq!(run_texts(&section), vec!["제안", "*", " 필요"]);
        let props = find_desc(&header, HH, "charProperties").unwrap();
        let prs = find_children(&props, HH, "charPr");
        assert_eq!(prs.len(), 2);
        let derived = &prs[1];
        assert_eq!(derived.borrow().get_attr("id").as_deref(), Some("8"));
        assert_eq!(derived.borrow().get_attr("textColor").as_deref(), Some("#FF0000"));
        assert!(find_child(derived, HH, "supscript").is_some());
        // 원본 charPr은 그대로다.
        assert_eq!(prs[0].borrow().get_attr("textColor").as_deref(), Some("#000000"));
        let star_run = find_all_desc(&section, HP, "run")
            .into_iter()
            .find(|r| run_text_of(r) == "*")
            .unwrap();
        assert_eq!(star_run.borrow().get_attr("charPrIDRef").as_deref(), Some("8"));
    }

    #[test]
    fn unattached_asterisk_is_left_alone() {
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>* 별표 주석</hp:t></hp:run>
<hp:run charPrIDRef="7"><hp:t>기호 ** 굵게</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="1"><hh:charPr id="7" height="1000" textColor="#000000"/></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(!apply_attached_asterisk(&section, &header).unwrap());
        assert_eq!(run_texts(&section), vec!["* 별표 주석", "기호 ** 굵게"]);
    }

    #[test]
    fn existing_superscript_char_pr_is_reused() {
        // 시그니처가 같은 charPr이 이미 있으면 새 ID를 만들지 않는다.
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>제안* 필요</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="2"><hh:charPr id="7" height="1000" textColor="#000000"/><hh:charPr id="9" height="1000" textColor="#FF0000"><hh:supscript/></hh:charPr></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_attached_asterisk(&section, &header).unwrap());
        let props = find_desc(&header, HH, "charProperties").unwrap();
        assert_eq!(find_children(&props, HH, "charPr").len(), 2);
        assert_eq!(find_children(&props, HH, "charPr")[1].borrow().get_attr("id").as_deref(), Some("9"));
        let star_run = find_all_desc(&section, HP, "run")
            .into_iter()
            .find(|r| run_text_of(r) == "*")
            .unwrap();
        assert_eq!(star_run.borrow().get_attr("charPrIDRef").as_deref(), Some("9"));
    }

    #[test]
    fn run_without_char_pr_ref_keeps_none_on_segments() {
        // charPrIDRef가 없는 run도 분할되며, 새 조각에 charPrIDRef가
        // 생기지 않는다(굵게 후처리의 기존 동작).
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run><hp:t>앞 **굵게** 뒤</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="0"></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_markdown_bold(&section, &header).unwrap());
        assert_eq!(run_texts(&section), vec!["앞 ", "굵게", " 뒤"]);
        for r in find_all_desc(&section, HP, "run") {
            assert!(r.borrow().get_attr("charPrIDRef").is_none());
        }
    }

    #[test]
    fn line_break_resets_attached_context() {
        // 줄바꿈 직후의 괄호는 '붙은' 것으로 본다(선행 문자가 없음).
        let section = parse(
            r##"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
<hp:p><hp:run charPrIDRef="7"><hp:t>첫줄</hp:t><hp:lineBreak/><hp:t>(둘째줄)</hp:t></hp:run></hp:p></hp:sec>"##
                .as_bytes(),
        )
        .unwrap();
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="1"><hh:charPr id="7" height="1000"/></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap();
        assert!(apply_parenthesis_size(&section, &header).unwrap());
        // lineBreak 전용 중간 run은 텍스트가 비어 보인다(기존 분할 동작).
        assert_eq!(run_texts(&section), vec!["첫줄", "", "(둘째줄)"]);
        let heights = char_pr_heights(&header);
        assert_eq!(heights.len(), 2);
        let shrunk = find_all_desc(&section, HP, "run")
            .into_iter()
            .find(|r| run_text_of(r) == "(둘째줄)")
            .unwrap()
            .borrow()
            .get_attr("charPrIDRef")
            .unwrap();
        assert_eq!(shrunk, heights[1].0);
    }
}
