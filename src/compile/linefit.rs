//! 휴먼명조 15pt depth 블록의 한 줄 폭 자동 맞춤.
//!
//! depth 문단은 한 줄(48188 HWP unit = 481.88pt) 안에
//! "  □ (키워드) 본문..."이 들어간다. 휴먼명조의 한글 1글자는 1em(15pt)이고
//! 자간 -n%는 글자 사이 간격을 n/50em 줄이므로, 초과 폭을 자간으로 상쇄할
//! 수 있다. 초과 1~6글자를 2→1줄/3→2줄로 나눠 경험식으로 계산한다.
//!
//! 비용 구조: 호출당 문단 수에 선형인 단일 패스다. 15pt charPr 집합과 charPr
//! 변형 시그니처 캐시는 루프 밖에서 한 번만 만들므로, 문단 종류가
//! depth-2 ↔ asterisk ↔ depth-2 처럼 섞여 왕복해도 문단마다 재초기화가
//! 없고 캐시가 반복 원형을 그대로 재사용한다.

use std::collections::{HashMap, HashSet};

use crate::compile::metrics::text_width;
use crate::shared::error::{AppError, Result};
use crate::shared::xmltree::{
    deep_clone, find_child, find_children, find_desc, iter_elements, serialize_signature, set_attr,
    HH, HP, NodeRef,
};

/// 한 줄 폭(HWP unit). template.hwpx secPr/lineseg의 48188.
const LINE_WIDTH_UNITS: i64 = 48_188;

/// 자간 -1%가 문단에서 절약하는 폭(HWP unit). 간격 1개 = 1/50em = 30 unit.
const UNIT_PER_PERCENT: f64 = 1500.0 / 50.0;

/// 보정 대상 초과 글자 수 상한(한글 기준). 그 이상은 자간으로 못 넣는다.
const MAX_OVER_CHARS: i64 = 6;

/// 자간 축소 상한(%). 그 이상은 미학적으로 무리다.
const MAX_PERCENT: i64 = 12;

/// 문단 텍스트의 폭 초과분(HWP unit). 경계 폭을 넘은 만큼.
fn overflow_units(text: &str, boundary_units: i64) -> Option<i64> {
    let w = text_width(text);
    if w <= boundary_units {
        return None;
    }
    Some(w - boundary_units)
}

/// 초과 폭을 상쇄하는 데 필요한 자간(%). 문단 전체 글자 간격 수를 쓴다.
/// -1%당 간격 하나가 30 unit 줄고, 간격 수 = (문자 수 - 1).
/// 경험 보정계수 1.05(5% 여유)를 곱하고 올림한다.
fn needed_spacing_percent(overflow: i64, total_chars: usize) -> i64 {
    if total_chars < 2 {
        return 0;
    }
    let gaps = (total_chars - 1) as f64;
    let per_percent = gaps * UNIT_PER_PERCENT;
    let percent = (overflow as f64 / per_percent * 1.05).ceil() as i64;
    percent.min(MAX_PERCENT)
}

/// id 속성을 제외한 charPr 직렬화 시그니처.
fn char_pr_signature(char_pr: &NodeRef) -> Vec<u8> {
    let clone = deep_clone(char_pr);
    clone.borrow_mut().remove_attr("id");
    serialize_signature(&clone)
}

/// 문단의 가시 텍스트(hp:t 전부, linesegarray 제외)를 모은다.
fn paragraph_text(paragraph: &NodeRef) -> String {
    fn walk(node: &NodeRef, out: &mut String) {
        for child in &node.borrow().children {
            let b = child.borrow();
            if b.is(HP, "t") {
                out.push_str(&b.text_or_empty());
            } else if !b.is(HP, "linesegarray") {
                walk(&child.clone(), out);
            }
        }
    }
    let mut out = String::new();
    walk(paragraph, &mut out);
    out
}

/// 컴파일 후처리: 15pt(휴먼명조) 문단이 줄 경계를 1~6글자 살짝 넘칠 때
/// 자간을 줄여 줄 수를 유지한다. depth1(□)/depth2(○)/depth3 전 레벨이
/// 대상이다 — 세 레벨 모두 15pt 문단이고 들여쓰기 prefix 폭만 다르므로,
/// 경계는 문단 폭 계산에 prefix가 포함되어 자동으로 반영된다.
/// 2→1줄(1줄 경계 초과)과 3→2줄(2줄 경계 초과)을 각각 판정한다.
///
/// 상태는 모두 루프 밖에서 한 번 구축한다:
/// - `fit_ids`: height=1500 charPr id 집합(문단 판정 O(1) 조회)
/// - `by_signature`/`next_id`: charPr 변형 캐시(같은 변형 재사용, 중복 생성 방지)
pub fn apply_line_fit(section_root: &NodeRef, header_root: &NodeRef) -> Result<bool> {
    // charProperties가 없는(빈 문서·일반 섹션) header는 보정 대상이 없으므로
    // 오류가 아니라 HEAD의 is_15pt_paragraph=false 경로처럼 Ok(false)로 통과한다.
    let Some(props) = find_desc(header_root, HH, "charProperties") else {
        return Ok(false);
    };
    let char_prs = find_children(&props, HH, "charPr");

    // 15pt charPr id 집합 — 루프당 전수 스캔 대신 1회 구축.
    let fit_ids: HashSet<String> = char_prs
        .iter()
        .filter(|cp| cp.borrow().get_attr("height").as_deref() == Some("1500"))
        .filter_map(|cp| cp.borrow().get_attr("id"))
        .collect();

    // 변형 캐시: 기존 정의 시그니처 → id. 변형을 새로 만들 때도 여기 등록해
    // 이후 문단이 같은 (base, percent)를 재사용하게 한다.
    let mut by_signature: HashMap<Vec<u8>, String> = char_prs
        .iter()
        .map(|cp| (char_pr_signature(cp), cp.borrow().get_attr("id").unwrap_or_default()))
        .collect();
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
    let mut item_count = char_prs.len();

    let mut changed = false;
    for paragraph in iter_elements(section_root) {
        if !paragraph.borrow().is(HP, "p") {
            continue;
        }
        let has_fit_run = find_children(&paragraph, HP, "run")
            .iter()
            .filter_map(|r| r.borrow().get_attr("charPrIDRef"))
            .any(|id| fit_ids.contains(&id));
        if !has_fit_run {
            continue;
        }
        let text = paragraph_text(&paragraph);
        if text.trim().is_empty() {
            continue;
        }
        // 2→1줄: 1줄 폭 초과 / 3→2줄: 2줄 폭 초과.
        for boundary in [LINE_WIDTH_UNITS, LINE_WIDTH_UNITS * 2] {
            let overflow = match overflow_units(&text, boundary) {
                Some(o) => o,
                None => continue,
            };
            // 초과분이 한글 1~6글자 분량이어야 보정한다.
            if overflow > MAX_OVER_CHARS * 1500 {
                continue; // 이 경계에선 못 넣는다. 더 큰 경계(다음 줄 수)를 본다
            }
            let percent = needed_spacing_percent(overflow, text.chars().count());
            if percent <= 0 {
                continue;
            }
            let runs = find_children(&paragraph, HP, "run");
            let base_ids: Vec<String> = {
                // base id는 run 등장 순서를 유지한다 — 변형 charPr id 배정이
                // 결정적이어야 같은 입력이 항상 같은 바이트를 낳는다(회귀 계약).
                let mut seen: HashSet<String> = HashSet::new();
                let mut ordered: Vec<String> = Vec::new();
                for r in &runs {
                    if let Some(id) = r.borrow().get_attr("charPrIDRef") {
                        if seen.insert(id.clone()) {
                            ordered.push(id);
                        }
                    }
                }
                ordered
            };
            if base_ids.is_empty() {
                continue;
            }
            // base charPr의 hangul/hanja spacing을 -percent로 바꾼 변형 id를
            // 얻는다. 캐시 조회로 같은 변형은 두 번 만들지 않는다.
            let mut derived: HashMap<String, String> = HashMap::new();
            for base_id in &base_ids {
                let base = char_prs
                    .iter()
                    .find(|cp| cp.borrow().get_attr("id").as_deref() == Some(base_id.as_str()))
                    .ok_or_else(|| {
                        AppError(format!("charPrIDRef={base_id} not found in header.xml."))
                    })?;
                let variant = deep_clone(base);
                set_attr(&variant, "id", "");
                let spacing = find_child(&variant, HH, "spacing")
                    .ok_or_else(|| AppError(format!("charPrIDRef={base_id} has no hh:spacing.")))?;
                let v = (-percent).to_string();
                set_attr(&spacing, "hangul", &v);
                set_attr(&spacing, "hanja", &v);
                let signature = char_pr_signature(&variant);
                let id = match by_signature.get(&signature) {
                    Some(existing) if !existing.is_empty() => existing.clone(),
                    _ => {
                        let id = next_id.to_string();
                        next_id += 1;
                        variant.borrow_mut().set_attr("id", &id);
                        props.borrow_mut().children.push(variant.clone());
                        by_signature.insert(signature, id.clone());
                        item_count += 1;
                        id
                    }
                };
                derived.insert(base_id.clone(), id);
            }
            for run in &runs {
                let base = run.borrow().get_attr("charPrIDRef");
                if let Some(new_id) = base.as_ref().and_then(|b| derived.get(b)) {
                    set_attr(run, "charPrIDRef", new_id);
                }
            }
            changed = true;
            break;
        }
    }
    if changed {
        set_attr(&props, "itemCnt", &item_count.to_string());
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::xmltree::{find_all_desc, parse};

    /// 한글 33글자 = 49500 unit → 1줄 경계(48188)를 1312 unit 넘는 본문.
    fn overflow_text() -> String {
        "가".repeat(33)
    }

    fn section_with(paragraphs: &[String]) -> NodeRef {
        let xml = format!(
            r#"<hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">{}</hp:sec>"#,
            paragraphs.join("\n")
        );
        parse(xml.as_bytes()).unwrap()
    }

    fn paragraph(char_pr_id: &str, text: &str) -> String {
        format!(
            r#"<hp:p><hp:run charPrIDRef="{char_pr_id}"><hp:t>{text}</hp:t></hp:run></hp:p>"#
        )
    }

    /// 15pt(id=1)와 10pt(id=2) charPr을 가진 header.
    fn header_with_two_char_prs() -> NodeRef {
        parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
<hh:charProperties itemCnt="2"><hh:charPr id="1" height="1500"><hh:spacing hangul="0" hanja="0"/></hh:charPr><hh:charPr id="2" height="1000"><hh:spacing hangul="0" hanja="0"/></hh:charPr></hh:charProperties></hh:hh>"##
                .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn missing_char_properties_empty_section_is_ok_false() {
        // charProperties가 아예 없는 header + 빈 섹션: 오류가 아니라 보정 없이 통과한다.
        let section = section_with(&[]);
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"/>"##.as_bytes(),
        )
        .unwrap();
        assert_eq!(apply_line_fit(&section, &header).unwrap(), false);
    }

    #[test]
    fn missing_char_properties_with_paragraph_run_is_ok_false() {
        // 문단과 run이 있어도 charProperties가 없으면 보정 없이 Ok(false)다.
        let section = section_with(&[paragraph("7", &overflow_text())]);
        let header = parse(
            r##"<hh:hh xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"/>"##.as_bytes(),
        )
        .unwrap();
        assert_eq!(apply_line_fit(&section, &header).unwrap(), false);
        // header에 charProperties를 몰래 만들어내지도 않는다.
        assert!(find_desc(&header, HH, "charProperties").is_none());
    }

    #[test]
    fn alternating_15pt_and_plain_paragraphs_fit_and_reuse() {
        // 15pt ↔ 10pt ↔ 15pt 처럼 섞여 왕복해도 앞 문단의 변형을 그대로 재사용한다.
        let text = overflow_text();
        let section = section_with(&[
            paragraph("1", &text),
            paragraph("2", &text),
            paragraph("1", &text),
        ]);
        let header = header_with_two_char_prs();
        assert!(apply_line_fit(&section, &header).unwrap());
        let runs = find_all_desc(&section, HP, "run");
        assert_eq!(runs.len(), 3);
        // 첫 15pt 문단은 다음 빈 id=3 변형으로 바뀐다.
        assert_eq!(
            runs[0].borrow().get_attr("charPrIDRef").as_deref(),
            Some("3")
        );
        // 10pt 문단은 그대로다.
        assert_eq!(
            runs[1].borrow().get_attr("charPrIDRef").as_deref(),
            Some("2")
        );
        // 뒤따르는 15pt 문단도 같은 변형을 재사용한다(중복 생성 없음).
        assert_eq!(
            runs[2].borrow().get_attr("charPrIDRef").as_deref(),
            Some("3")
        );
        let props = find_desc(&header, HH, "charProperties").unwrap();
        assert_eq!(find_children(&props, HH, "charPr").len(), 3);
        assert_eq!(props.borrow().get_attr("itemCnt").as_deref(), Some("3"));
    }

    #[test]
    fn repeated_same_variant_is_created_once() {
        let text = overflow_text();
        let section = section_with(&[paragraph("1", &text), paragraph("1", &text)]);
        let header = header_with_two_char_prs();
        assert!(apply_line_fit(&section, &header).unwrap());
        let runs = find_all_desc(&section, HP, "run");
        let first = runs[0].borrow().get_attr("charPrIDRef");
        let second = runs[1].borrow().get_attr("charPrIDRef");
        // 같은 (base, percent)를 두 번 만들지 않고 하나의 변형을 재사용한다.
        assert_eq!(first, second);
        let props = find_desc(&header, HH, "charProperties").unwrap();
        let prs = find_children(&props, HH, "charPr");
        assert_eq!(prs.len(), 3);
        assert_eq!(props.borrow().get_attr("itemCnt").as_deref(), Some("3"));
        // 변형은 base(id=1, 15pt)의 자간만 -2로 바꾼 것이다.
        let variant = &prs[2];
        assert_eq!(variant.borrow().get_attr("id").as_deref(), Some("3"));
        assert_eq!(variant.borrow().get_attr("height").as_deref(), Some("1500"));
        let spacing = find_child(variant, HH, "spacing").unwrap();
        assert_eq!(spacing.borrow().get_attr("hangul").as_deref(), Some("-2"));
        assert_eq!(spacing.borrow().get_attr("hanja").as_deref(), Some("-2"));
    }

    #[test]
    fn mixed_base_run_ids_get_distinct_variants() {
        // 한 문단에 base가 다른 run이 섞여 있으면 run 등장 순서대로 각각 변형을 얻는다.
        // 17글자(25500) + 16글자(24000) = 49500 unit → 초과 1312 unit.
        let section = section_with(&[format!(
            r#"<hp:p><hp:run charPrIDRef="1"><hp:t>{}</hp:t></hp:run><hp:run charPrIDRef="2"><hp:t>{}</hp:t></hp:run></hp:p>"#,
            "가".repeat(17),
            "가".repeat(16)
        )]);
        let header = header_with_two_char_prs();
        assert!(apply_line_fit(&section, &header).unwrap());
        let runs = find_all_desc(&section, HP, "run");
        assert_eq!(runs.len(), 2);
        // 15pt base(id=1) → 변형 id=3, 10pt base(id=2) → 변형 id=4.
        assert_eq!(
            runs[0].borrow().get_attr("charPrIDRef").as_deref(),
            Some("3")
        );
        assert_eq!(
            runs[1].borrow().get_attr("charPrIDRef").as_deref(),
            Some("4")
        );
        let props = find_desc(&header, HH, "charProperties").unwrap();
        let prs = find_children(&props, HH, "charPr");
        assert_eq!(prs.len(), 4);
        assert_eq!(props.borrow().get_attr("itemCnt").as_deref(), Some("4"));
        // 각 변형은 자기 base의 height를 물려받는다.
        assert_eq!(prs[2].borrow().get_attr("height").as_deref(), Some("1500"));
        assert_eq!(prs[3].borrow().get_attr("height").as_deref(), Some("1000"));
    }
}
