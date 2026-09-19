use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use regex::NoExpand;

use crate::shared::error::{err, AppError, Result};
use crate::compile::patterns::{TEMPLATE_DATE_RE, TEMPLATE_MARKER_RE};
use crate::compile::render::render_block;
use crate::compile::source::Block;
use crate::shared::xmltree::{
    append_child, deep_clone, find_all_desc, find_children, find_desc,
    insert_element, iter_elements, ptr, remove_child, text_nodes, HP, NodeRef,
};

pub const DEPTH_KINDS: [&str; 3] = ["depth1", "depth2", "depth3"];
pub const NOTE_KINDS: [&str; 2] = ["asterisk", "reference"];

/// prototype/메모 범위에서 내용이 들어갈 자리를 나타내는 표식 텍스트.
/// 제목·소제목·박스 항목의 "샘플" 자리와 검증 단계의 남은 자리 경고가
/// 함께 본다.
pub const SAMPLE_TEXT: &str = "샘플";

pub fn is_depth_kind(kind: &str) -> bool {
    DEPTH_KINDS.contains(&kind)
}

pub fn is_note_kind(kind: &str) -> bool {
    NOTE_KINDS.contains(&kind)
}

/// 컴포넌트 종류별 필수 구성요소 메모 이름. 새 컴포넌트 종류는 여기 한
/// 곳에 추가하면 is_component_kind와 component_styles가 함께 따라간다.
fn component_requirements(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "box" => Some(&["title", "bullet", "body"]),
        k if is_note_kind(k) => Some(&["bullet", "body"]),
        k if is_depth_kind(k) => Some(&["bullet", "keyword", "body"]),
        _ => None,
    }
}

pub fn is_component_kind(kind: &str) -> bool {
    component_requirements(kind).is_some()
}

/// 템플릿 메모에서 읽은 prototype/component 카탈로그.
/// Vec 쌍은 Python dict의 삽입 순서(템플릿 순서)를 유지한다.
pub struct Catalog {
    pub title: Vec<(String, NodeRef)>,
    pub prototypes: Vec<(String, NodeRef)>,
    pub components: Vec<(String, Vec<(String, NodeRef)>)>,
}

fn upsert(list: &mut Vec<(String, NodeRef)>, key: &str, value: NodeRef) {
    match list.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => list.push((key.to_string(), value)),
    }
}

fn setdefault(list: &mut Vec<(String, NodeRef)>, key: &str, value: NodeRef) {
    if !list.iter().any(|(k, _)| k == key) {
        list.push((key.to_string(), value));
    }
}

/// 메모 범위 본문(subList) 안의 run은 문서 run에서 제외한다.
pub fn document_runs(root: &NodeRef) -> Vec<(NodeRef, Option<NodeRef>)> {
    fn walk(node: &NodeRef, paragraph: Option<&NodeRef>, out: &mut Vec<(NodeRef, Option<NodeRef>)>) {
        {
            let b = node.borrow();
            if b.is(HP, "fieldBegin") && b.get_attr("type").as_deref() == Some("MEMO") {
                return;
            }
            if b.is(HP, "run") {
                out.push((node.clone(), paragraph.cloned()));
                return;
            }
        }
        let next_paragraph: Option<NodeRef> = if node.borrow().is(HP, "p") {
            Some(node.clone())
        } else {
            paragraph.cloned()
        };
        let child_count = node.borrow().children.len();
        for i in 0..child_count {
            let child = node.borrow().children[i].clone();
            walk(&child, next_paragraph.as_ref(), out);
        }
    }
    let mut out = Vec::new();
    walk(root, None, &mut out);
    out
}

/// MEMO fieldBegin 안의 표식 텍스트를 읽는다.
fn memo_marker(field_begin: &NodeRef) -> Result<Option<(String, Option<String>)>> {
    let text: String = find_all_desc(field_begin, HP, "t")
        .iter()
        .map(|n| n.borrow().text_or_empty())
        .collect();
    let trimmed = text.trim();
    let c = match TEMPLATE_MARKER_RE.captures(trimmed) {
        Some(c) if c.get(0).unwrap().as_str() == trimmed => c,
        _ => return Ok(None),
    };
    let category = c[1].to_lowercase();
    if category == "heading_default" {
        return Ok(Some(("heading".to_string(), Some("default".to_string()))));
    }
    Ok(Some((category, c.get(2).map(|m| m.as_str().to_string()))))
}

/// MEMO fieldBegin subtree의 보이는 텍스트.
pub fn visible_text(element: &NodeRef) -> String {
    fn walk(node: &NodeRef, out: &mut String) {
        {
            let b = node.borrow();
            if b.is(HP, "fieldBegin") && b.get_attr("type").as_deref() == Some("MEMO") {
                return;
            }
            if b.is(HP, "t") {
                out.push_str(&b.text_or_empty());
                return;
            }
        }
        let child_count = node.borrow().children.len();
        for i in 0..child_count {
            let child = node.borrow().children[i].clone();
            walk(&child, out);
        }
    }
    let mut out = String::new();
    walk(element, &mut out);
    out
}

struct MemoEntry {
    category: String,
    name: Option<String>,
    runs: Vec<NodeRef>,
    /// 메모의 fieldBegin 요소. 표 셀 안의 메모는 커버 run이 document_runs에
    /// 나오지 않으므로(표를 감싼 run만 나옴) 이 노드로 실제 run을 찾는다.
    begin: NodeRef,
    start_paragraph: Option<NodeRef>,
    end_paragraph: Option<NodeRef>,
}

/// 메모 시작 표식이 표 앞 빈 문단에 붙는 템플릿 호환: 범위 끝에 표가 있으면
/// 그 쪽 문단을 prototype으로 택한다(heading/box).
fn prototype_with_table(entry: &MemoEntry, start: NodeRef) -> NodeRef {
    let start_has_tbl = find_desc(&start, HP, "tbl").is_some();
    let end_has_tbl = entry
        .end_paragraph
        .as_ref()
        .and_then(|p| find_desc(p, HP, "tbl"))
        .is_some();
    if !start_has_tbl && end_has_tbl {
        return entry.end_paragraph.clone().expect("end 문단이 있습니다");
    }
    start
}

/// 문단 안에서 fieldBegin 요소를 직접 자식으로 둔 run. 표 셀 안 메모의
/// 구성요소 run을 찾는다. 표를 감싼 바깥 run도 자손에 메모를 두므로
/// 문서 순서를 뒤집어 가장 안쪽 run(직접 부모)을 택한다.
fn run_covering_memo(paragraph: &NodeRef, begin: &NodeRef) -> Option<NodeRef> {
    find_all_desc(paragraph, HP, "run")
        .into_iter()
        .rev()
        .find(|r| iter_elements(r).iter().any(|e| ptr(e) == ptr(begin)))
}

/// HWP 메모 범위에서 prototype 문단과 문자 스타일 run을 읽는다.
pub fn read_template_catalog(root: &NodeRef) -> Result<Catalog> {
    let mut entries: Vec<MemoEntry> = Vec::new();
    let mut active: Vec<(String, MemoEntry)> = Vec::new();

    for (run, paragraph) in document_runs(root) {
        for child in iter_elements(&run) {
            let is_memo_begin = {
                let b = child.borrow();
                b.is(HP, "fieldBegin") && b.get_attr("type").as_deref() == Some("MEMO")
            };
            if is_memo_begin {
                if let Some((category, name)) = memo_marker(&child)? {
                    let id = child.borrow().get_attr("id").unwrap_or_default();
                    active.push((
                        id,
                        MemoEntry {
                            category,
                            name,
                            runs: vec![run.clone()],
                            begin: child.clone(),
                            start_paragraph: paragraph.clone(),
                            end_paragraph: paragraph.clone(),
                        },
                    ));
                }
            } else {
                let is_field_end = child.borrow().is(HP, "fieldEnd");
                if is_field_end {
                    let begin_id = child.borrow().get_attr("beginIDRef").unwrap_or_default();
                    if let Some(pos) = active.iter().position(|(k, _)| *k == begin_id) {
                        let (_, mut entry) = active.remove(pos);
                        if entry.runs.last().is_none_or(|r| !Rc::ptr_eq(r, &run)) {
                            entry.runs.push(run.clone());
                        }
                        entry.end_paragraph = paragraph.clone();
                        entries.push(entry);
                    }
                }
            }
        }
        for (_, entry) in active.iter_mut() {
            if entry.runs.last().is_none_or(|r| !Rc::ptr_eq(r, &run)) {
                entry.runs.push(run.clone());
                entry.end_paragraph = paragraph.clone();
            }
        }
    }
    entries.extend(active.into_iter().map(|(_, e)| e));

    let mut catalog = Catalog {
        title: Vec::new(),
        prototypes: Vec::new(),
        components: Vec::new(),
    };
    for entry in &entries {
        let category = entry.category.as_str();
        if category == "title" {
            let paragraph = entry
                .start_paragraph
                .clone()
                .expect("title memo에는 시작 문단이 있습니다");
            upsert(&mut catalog.title, entry.name.as_deref().unwrap_or(""), paragraph);
            continue;
        }
        if category == "heading" {
            let name = entry.name.clone().unwrap_or_else(|| "default".to_string());
            let start = entry
                .start_paragraph
                .clone()
                .expect("heading memo에는 시작 문단이 있습니다");
            let prototype = prototype_with_table(entry, start);
            upsert(
                &mut catalog.prototypes,
                &format!("heading_{}", name.to_lowercase()),
                prototype,
            );
            continue;
        }
        // 이름 없는 [[box]]는 표 전체를 덮는 prototype 표식이고, [[box:title]]
        // [[box:bullet]] [[box:body]]는 구성요소 run을 가리킨다.
        if category == "box" && entry.name.is_none() {
            let start = entry
                .start_paragraph
                .clone()
                .expect("box memo에는 시작 문단이 있습니다");
            let prototype = prototype_with_table(entry, start);
            upsert(&mut catalog.prototypes, "box", prototype);
            continue;
        }
        // Component memo는 문단 경계를 넘을 수 있다. 범위의 마지막 run이
        // 실제 선택된 구성요소(예: depth1:bullet)이다. 표 셀 안의 메모는
        // document_runs에 셀 run이 나오지 않으므로 fieldBegin을 감싸는
        // run을 따로 찾는다(box:title/bullet/body).
        let component_run = if category == "box" {
            let paragraph = entry
                .end_paragraph
                .clone()
                .expect("box 구성요소 메모에는 끝 문단이 있습니다");
            run_covering_memo(&paragraph, &entry.begin)
                .or_else(|| entry.runs.last().cloned())
                .expect("box 구성요소 run을 찾지 못했습니다")
        } else {
            entry.runs.last().cloned().expect("memo run은 비지 않습니다")
        };
        let slot = match catalog
            .components
            .iter_mut()
            .find(|(k, _)| k == category)
        {
            Some((_, v)) => v,
            None => {
                catalog.components.push((category.to_string(), Vec::new()));
                &mut catalog.components.last_mut().unwrap().1
            }
        };
        upsert(slot, entry.name.as_deref().unwrap_or(""), component_run);
        if let Some(end) = entry.end_paragraph.clone() {
            setdefault(&mut catalog.prototypes, category, end);
        }
    }
    Ok(catalog)
}

/// 요청한 변형을 반환하거나 템플릿 순서상 첫 변형을 돌려준다.
pub fn select_template_variant(
    variants: &[(String, NodeRef)],
    category: &str,
    requested: &HashMap<String, String>,
) -> Result<Option<NodeRef>> {
    if variants.is_empty() {
        if requested.contains_key(&category.to_lowercase()) {
            return err(format!("variant category not found in template: {category}"));
        }
        return Ok(None);
    }
    let wanted = match requested.get(&category.to_lowercase()) {
        Some(w) => w,
        None => return Ok(Some(variants[0].1.clone())),
    };
    for (name, paragraph) in variants {
        if name.to_lowercase() == wanted.to_lowercase() {
            return Ok(Some(paragraph.clone()));
        }
    }
    let available = variants
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    err(format!(
        "template variant not found: {category}={wanted} (available: {available})"
    ))
}

/// 복제된 prototype에서 메모 컨트롤과 표식 텍스트를 제거한다.
pub fn remove_variant_markers(element: &NodeRef) {
    for parent in iter_elements(element) {
        let to_remove: Vec<NodeRef> = parent
            .borrow()
            .children
            .iter()
            .filter(|child| {
                let cb = child.borrow();
                if !cb.is(HP, "ctrl") {
                    return false;
                }
                cb.children.iter().any(|x| {
                    let xb = x.borrow();
                    xb.is(HP, "fieldBegin") && xb.get_attr("type").as_deref() == Some("MEMO")
                }) || cb
                    .children
                    .iter()
                    .any(|x| x.borrow().is(HP, "fieldEnd"))
            })
            .cloned()
            .collect();
        for r in to_remove {
            remove_child(&parent, &r);
        }
    }
    for node in text_nodes(element) {
        let has_text = node.borrow().text.as_ref().is_some_and(|t| !t.is_empty());
        if has_text {
            let mut nb = node.borrow_mut();
            let replaced = TEMPLATE_MARKER_RE
                .replace_all(nb.text.as_deref().unwrap(), "")
                .into_owned();
            nb.text = Some(replaced);
        }
    }
}

/// ’26.9.6(일) 형식의 한국어 짧은 날짜 문자열.
pub fn today_korean() -> String {
    use chrono::{Datelike, Local};
    const WEEKDAYS: [&str; 7] = ["월", "화", "수", "목", "금", "토", "일"];
    let d = Local::now().date_naive();
    let idx = d.weekday().num_days_from_monday() as usize;
    let yy = d.year().rem_euclid(100);
    format!("’{:02}.{}.{}({})", yy, d.month(), d.day(), WEEKDAYS[idx])
}

/// 선택된 제목 템플릿의 한국어 짧은 날짜를 오늘로 바꾼다.
pub fn replace_template_dates(element: &NodeRef) {
    let formatted = today_korean();
    for node in text_nodes(element) {
        let has_text = node.borrow().text.as_ref().is_some_and(|t| !t.is_empty());
        if has_text {
            let mut nb = node.borrow_mut();
            let replaced = TEMPLATE_DATE_RE
                .replace_all(nb.text.as_deref().unwrap(), NoExpand(&formatted))
                .into_owned();
            nb.text = Some(replaced);
        }
    }
}

/// 표지 표의 제목 셀을 찾아 텍스트를 쓴다. 샘플 placeholder가 있는 셀을
/// 우선하고, 없으면 텍스트 노드를 가진 첫 셀을 대신 쓴다(attachment 표는
/// placeholder가 마지막 셀에 있다).
fn write_title_cell(table: &NodeRef, text: &str) -> Result<()> {
    let cells = find_all_desc(table, HP, "tc");
    let target = cells
        .iter()
        .find(|cell| {
            text_nodes(cell)
                .iter()
                .any(|n| n.borrow().text.as_deref().is_some_and(|t| t.contains("샘플")))
        })
        .or_else(|| cells.iter().find(|cell| !text_nodes(cell).is_empty()))
        .or_else(|| cells.first());
    let Some(cell) = target else {
        return err("title table cell not found in template.hwpx.");
    };
    let nodes = text_nodes(cell);
    if nodes.is_empty() {
        return err("no text node in the title table cell.");
    }
    nodes[0].borrow_mut().set_text(text);
    for node in &nodes[1..] {
        node.borrow_mut().set_text("");
    }
    Ok(())
}

fn component_styles(catalog: &Catalog, kind: &str) -> Result<HashMap<String, String>> {
    let values = catalog
        .components
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v);
    let required = component_requirements(kind)
        .expect("component_styles는 is_component_kind 종류에만 호출된다");
    let mut missing = Vec::new();
    for name in required {
        let found = values
            .and_then(|v| v.iter().find(|(n, _)| n == name))
            .is_some();
        if !found {
            missing.push(format!("[[{kind}:{name}]]"));
        }
    }
    if !missing.is_empty() {
        return err(format!(
            "component memo not found in template.hwpx: {}",
            missing.join(", ")
        ));
    }
    let mut styles = HashMap::new();
    for name in required {
        let run = values
            .and_then(|v| v.iter().find(|(n, _)| n == name))
            .map(|(_, r)| r)
            .unwrap();
        match run.borrow().get_attr("charPrIDRef") {
            Some(id) => {
                styles.insert(name.to_string(), id);
            }
            None => {
                return err(format!(
                    "no charPrIDRef on the [[{kind}:...]] memo run in template.hwpx."
                ));
            }
        }
    }
    // 글머리 기호는 component run의 첫 비공백 문자다. 메모가 하나의 run에
    // 인라인으로 있으면 그 run이 문단 전체에 걸치므로, 이 방식이 표식이
    // 라벨과 본문을 삼키지도 않게 해준다.
    let bullet_run = values
        .and_then(|v| v.iter().find(|(n, _)| n == "bullet"))
        .map(|(_, r)| r)
        .unwrap();
    let bullet_full = visible_text(bullet_run);
    let bullet_text = bullet_full
        .trim()
        .chars()
        .next()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_string())
        .unwrap_or_default();
    if bullet_text.is_empty() {
        return err(format!(
            "no bullet character on the [[{kind}:bullet]] memo run in template.hwpx."
        ));
    }
    styles.insert("bullet_text".to_string(), bullet_text);
    Ok(styles)
}

/// section0 직계 `hp:p` 문단 목록과 ptr→위치 색인.
struct DirectParagraphs {
    list: Vec<NodeRef>,
    index: HashMap<usize, usize>,
}

impl DirectParagraphs {
    fn of(root: &NodeRef) -> Self {
        let list = find_children(root, HP, "p");
        let index = list.iter().enumerate().map(|(i, p)| (ptr(p), i)).collect();
        DirectParagraphs { list, index }
    }

    fn len(&self) -> usize {
        self.list.len()
    }

    fn position(&self, p: &NodeRef) -> Result<usize> {
        self.index
            .get(&ptr(p))
            .copied()
            .ok_or_else(|| AppError("prototype paragraph is not a direct child of section0.".into()))
    }
}

/// prototype 문단의 복제본. 결과는 이 복제본에서 만들어진다.
fn deep_copied_prototypes(catalog: &Catalog) -> Vec<(String, NodeRef)> {
    catalog
        .prototypes
        .iter()
        .map(|(k, p)| (k.clone(), deep_clone(p)))
        .collect()
}

/// prototype 문단 바로 앞의 빈 문단을 간격 접두사로 복제한다. prototype이
/// 연달아 있으면 그 앞의 빈 문단으로 물러난다.
fn interval_prefixes(root: &NodeRef, catalog: &Catalog) -> Result<Vec<(String, NodeRef)>> {
    let direct = DirectParagraphs::of(root);
    let mut prototype_ids: HashSet<usize> =
        catalog.prototypes.iter().map(|(_, p)| ptr(p)).collect();
    prototype_ids.extend(catalog.title.iter().map(|(_, p)| ptr(p)));
    let mut prefixes = Vec::new();
    for (kind, paragraph) in &catalog.prototypes {
        let mut index = direct.position(paragraph)?;
        while index > 0 && prototype_ids.contains(&ptr(&direct.list[index - 1])) {
            index -= 1;
        }
        if index != 0 {
            prefixes.push((kind.clone(), deep_clone(&direct.list[index - 1])));
        }
    }
    Ok(prefixes)
}

/// title 변형 문단을 골라 메모/날짜를 정리하고 root에 끼워 넣는다. 다른
/// title 변형 문단은 제거된다. (선택된 문단, metadata 문단)을 반환한다.
fn install_title(
    root: &NodeRef,
    catalog: &Catalog,
    direct: &DirectParagraphs,
    requested: &HashMap<String, String>,
) -> Result<(Option<NodeRef>, Option<NodeRef>)> {
    let title_choices = &catalog.title;
    if title_choices.is_empty() {
        return Ok((None, None));
    }
    let selected_source = select_template_variant(title_choices, "title", requested)?
        .expect("title choices가 비지 않으면 변형이 있습니다");
    let positions = title_choices
        .iter()
        .map(|(_, p)| direct.position(p))
        .collect::<Result<Vec<_>>>()?;
    let selected_index = direct.position(&selected_source)?;
    let next_title_index = title_choices
        .iter()
        .zip(&positions)
        .filter(|((_, p), _)| !Rc::ptr_eq(p, &selected_source))
        .map(|(_, &i)| i)
        .min()
        .unwrap_or(direct.len());
    let mut title_metadata = None;
    if selected_index + 1 < next_title_index && selected_index + 1 < direct.len() {
        let meta = direct.list[selected_index + 1].clone();
        remove_variant_markers(&meta);
        replace_template_dates(&meta);
        title_metadata = Some(meta);
    }
    let selected = deep_clone(&selected_source);
    remove_variant_markers(&selected);
    replace_template_dates(&selected);
    let first_index = positions.iter().copied().min().unwrap();
    for (_, p) in title_choices {
        remove_child(root, p);
    }
    insert_element(root, first_index, selected.clone());
    Ok((Some(selected), title_metadata))
}

/// 제목 변형이 정의되지 않은 템플릿이면 첫 표를 담은 문단을 제목으로 쓴다.
fn fallback_title_paragraph(root: &NodeRef) -> Option<NodeRef> {
    find_children(root, HP, "p")
        .into_iter()
        .find(|p| find_desc(p, HP, "tbl").is_some())
}

/// 공통 heading prototype만 있을 때 [[heading]] 블록을 재지정한다. 일부
/// 템플릿은 번호 있는 상위 제목용 [[heading]] prototype이 없다.
fn apply_heading_fallback(blocks: &mut [Block], prototypes: &[(String, NodeRef)]) {
    let has_heading = prototypes.iter().any(|(k, _)| k == "heading");
    let has_default = prototypes.iter().any(|(k, _)| k == "heading_default");
    if !has_heading && has_default {
        for b in blocks.iter_mut() {
            if b.0 == "heading" {
                b.0 = "heading_default".to_string();
            }
        }
    }
}

fn ensure_prototypes_exist(prototypes: &[(String, NodeRef)], blocks: &[Block]) -> Result<()> {
    for (kind, _) in blocks {
        if !prototypes.iter().any(|(k, _)| k == kind) {
            return err(format!("[[{kind}]] prototype memo not found in template.hwpx."));
        }
    }
    Ok(())
}

/// 선택된 title 문단과 metadata 문단만 남기고 직계 문단을 치운다. 생성
/// 내용은 memo로 선택된 prototype에서 복제되어 뒤에 붙는다.
fn prune_direct_paragraphs(
    root: &NodeRef,
    selected: &Option<NodeRef>,
    metadata: &Option<NodeRef>,
) {
    for paragraph in find_children(root, HP, "p") {
        let keep = selected.as_ref().is_some_and(|s| Rc::ptr_eq(s, &paragraph))
            || metadata.as_ref().is_some_and(|m| Rc::ptr_eq(m, &paragraph));
        if !keep {
            remove_child(root, &paragraph);
        }
    }
}

/// 블록에 필요한 component 스타일을 모은다. 블록 순서대로 보며 중복은 건너뛴다.
fn collect_component_styles(
    catalog: &Catalog,
    blocks: &[Block],
) -> Result<HashMap<String, HashMap<String, String>>> {
    let mut styles = HashMap::new();
    for (kind, _) in blocks {
        if is_component_kind(kind) && !styles.contains_key(kind) {
            styles.insert(kind.clone(), component_styles(catalog, kind)?);
        }
    }
    Ok(styles)
}

fn render_blocks(
    root: &NodeRef,
    prototypes: &[(String, NodeRef)],
    prefixes: &[(String, NodeRef)],
    blocks: &[Block],
    styles: &HashMap<String, HashMap<String, String>>,
) -> Result<()> {
    for (kind, value) in blocks {
        let prefix = prefixes
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, p)| p)
            .ok_or_else(|| AppError(format!("no empty spacer paragraph before {kind} in template.hwpx.")))?;
        append_child(root, deep_clone(prefix));
        let prototype = prototypes
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, p)| p)
            .expect("blocks의 kind는 prototypes에 있음이 확인됨");
        let paragraph = deep_clone(prototype);
        render_block(kind, &paragraph, value, styles)?;
        append_child(root, paragraph);
    }
    Ok(())
}

/// `major` 같은 별칭을 표준 kind로 바꾼 복사본을 만든다.
fn normalize_blocks(blocks: &[(String, String)]) -> Vec<Block> {
    blocks
        .iter()
        .map(|(k, v)| {
            (
                if k == "major" { "heading".to_string() } else { k.clone() },
                v.clone(),
            )
        })
        .collect()
}

/// 템플릿 문단을 복제해 블록 내용으로 채워 새 section 루트를 만든다.
/// title_override는 [[붙임]]/[[외부제목]] 명령이 지정한 표지 셀 텍스트다.
pub fn build_section(
    template_root: &NodeRef,
    title: &str,
    blocks: &[(String, String)],
    requested_variants: &HashMap<String, String>,
    title_override: Option<&str>,
) -> Result<NodeRef> {
    let root = deep_clone(template_root);
    let mut requested: HashMap<String, String> = HashMap::new();
    for (k, v) in requested_variants {
        requested.insert(k.to_lowercase(), v.clone());
    }
    let catalog = read_template_catalog(&root)?;
    let mut blocks = normalize_blocks(blocks);
    let prototypes = deep_copied_prototypes(&catalog);
    let prefixes = interval_prefixes(&root, &catalog)?;
    let direct = DirectParagraphs::of(&root);
    let (selected_title, title_metadata) = install_title(&root, &catalog, &direct, &requested)?;
    let selected_title = selected_title.or_else(|| fallback_title_paragraph(&root));
    // 선택된 표지 문단의 표에 제목을 쓴다. 명령 텍스트가 없으면 문서 제목.
    // tables[0]이 아니라 선택된 표를 건드려야 비선택 변형이 지워진 뒤에도 남는다.
    let title_table = selected_title
        .as_ref()
        .and_then(|p| find_desc(p, HP, "tbl"))
        .or_else(|| find_all_desc(&root, HP, "tbl").into_iter().next());
    match title_table {
        Some(table) => write_title_cell(&table, title_override.unwrap_or(title))?,
        None => return err("title table not found in template.hwpx."),
    }
    apply_heading_fallback(&mut blocks, &prototypes);
    ensure_prototypes_exist(&prototypes, &blocks)?;
    for (_, p) in &prototypes {
        remove_variant_markers(p);
    }
    for (_, p) in &prefixes {
        remove_variant_markers(p);
    }
    prune_direct_paragraphs(&root, &selected_title, &title_metadata);
    let styles = collect_component_styles(&catalog, &blocks)?;
    render_blocks(&root, &prototypes, &prefixes, &blocks, &styles)?;
    Ok(root)
}
