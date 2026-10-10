//! 휴먼명조 15pt depth2 블록의 한 줄 폭 자동 맞춤.
//!
//! 템플릿의 depth2 문단은 48188 HWP unit(=481.88pt) 한 줄 안에
//! "  □ (키워드) 본문..."이 들어간다. 휴먼명조의 한글 1글자는 1em(15pt)이고
//! 자간 -n%는 글자 사이 간격을 n/50em 줄이므로, 초과 폭을 자간으로 상쇄할
//! 수 있다. 초과 1~4글자를 2→1줄/3→2줄로 나눠 경험식으로 계산한다.

use std::collections::{HashMap, HashSet};

use crate::compile::metrics::{char_width, text_width};
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
const MAX_OVER_CHARS: i64 = 4;

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

/// base charPr의 hangul/hanja spacing을 `percent`(음수)로 바꾼 변형 ID.
/// 시그니처가 같은 기존 charPr은 재사용한다.
fn tightened_char_pr_ids(
    header_root: &NodeRef,
    base_ids: &[String],
    percent: i64,
) -> Result<HashMap<String, String>> {
    let char_properties = find_desc(header_root, HH, "charProperties")
        .ok_or_else(|| AppError("header.xml has no charProperties.".into()))?;
    let char_prs = find_children(&char_properties, HH, "charPr");
    let mut by_signature: HashMap<Vec<u8>, String> = HashMap::new();
    for cp in &char_prs {
        by_signature.insert(
            char_pr_signature(cp),
            cp.borrow().get_attr("id").unwrap_or_default(),
        );
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

    let mut derived = HashMap::new();
    for base_id in base_ids {
        let base = char_prs
            .iter()
            .find(|cp| cp.borrow().get_attr("id").as_deref() == Some(base_id.as_str()))
            .ok_or_else(|| AppError(format!("charPrIDRef={base_id} not found in header.xml.")))?;
        let variant = deep_clone(base);
        set_attr(&variant, "id", &next_id.to_string());
        let spacing = find_child(&variant, HH, "spacing")
            .ok_or_else(|| AppError(format!("charPrIDRef={base_id} has no hh:spacing.")))?;
        let v = percent.to_string();
        set_attr(&spacing, "hangul", &v);
        set_attr(&spacing, "hanja", &v);
        let signature = char_pr_signature(&variant);
        let id = match by_signature.get(&signature) {
            Some(existing) => existing.clone(),
            None => {
                let id = next_id.to_string();
                next_id += 1;
                char_properties.borrow_mut().children.push(variant.clone());
                by_signature.insert(signature, id.clone());
                id
            }
        };
        derived.insert(base_id.clone(), id);
    }
    let count = find_children(&char_properties, HH, "charPr").len();
    set_attr(&char_properties, "itemCnt", &count.to_string());
    Ok(derived)
}

/// 문단의 가시 텍스트(MEMO 마커 제외 hp:t 전부)를 모은다.
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

/// 휴먼명조 15pt(charPr height=1500) run이 있는 문단인지.
fn is_15pt_paragraph(paragraph: &NodeRef, header_root: &NodeRef) -> bool {
    let ids: HashSet<String> = find_children(paragraph, HP, "run")
        .iter()
        .filter_map(|r| r.borrow().get_attr("charPrIDRef"))
        .collect();
    if ids.is_empty() {
        return false;
    }
    let props = match find_desc(header_root, HH, "charProperties") {
        Some(p) => p,
        None => return false,
    };
    for cp in find_children(&props, HH, "charPr") {
        let id = match cp.borrow().get_attr("id") {
            Some(i) => i,
            None => continue,
        };
        if ids.contains(&id) && cp.borrow().get_attr("height").as_deref() == Some("1500") {
            return true;
        }
    }
    false
}

/// 컴파일 후처리: 15pt(휴먼명조) 문단이 줄 경계를 1~4글자 살짝 넘칠 때
/// 자간을 줄여 줄 수를 유지한다. 2→1줄(1줄 경계 초과)과 3→2줄(2줄 경계
/// 초과)을 각각 판정한다. 문단당 한 번, 가장 큰 경계 기준.
pub fn apply_line_fit(section_root: &NodeRef, header_root: &NodeRef) -> Result<bool> {
    let mut changed = false;
    for paragraph in iter_elements(section_root) {
        if !paragraph.borrow().is(HP, "p") {
            continue;
        }
        if !is_15pt_paragraph(&paragraph, header_root) {
            continue;
        }
        let text = paragraph_text(&paragraph);
        if text.trim().is_empty() {
            continue;
        }
        // 2→1줄: 1줄 폭 초과 / 3→2줄: 2줄 폭 초과.
        for (index, boundary) in [
            (0usize, LINE_WIDTH_UNITS),
            (1, LINE_WIDTH_UNITS * 2),
        ] {
            let overflow = match overflow_units(&text, boundary) {
                Some(o) => o,
                None => continue,
            };
            // 초과분이 한글 1~4글자 분량이어야 보정한다.
            if overflow > MAX_OVER_CHARS * 1500 {
                continue; // 이 경계에선 못 넣는다. 더 큰 경계(다음 줄 수)를 본다
            }
            let percent = needed_spacing_percent(overflow, text.chars().count());
            if percent <= 0 {
                continue;
            }
            let runs = find_children(&paragraph, HP, "run");
            let base_ids: Vec<String> = {
                let mut seen = HashSet::new();
                runs.iter()
                    .filter_map(|r| r.borrow().get_attr("charPrIDRef"))
                    .filter(|id| seen.insert(id.clone()))
                    .collect()
            };
            if base_ids.is_empty() {
                continue;
            }
            let derived = tightened_char_pr_ids(header_root, &base_ids, -percent)?;
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
    Ok(changed)
}
