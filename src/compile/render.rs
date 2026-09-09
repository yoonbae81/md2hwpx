use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

use crate::shared::error::{err, AppError, Result};
use crate::shared::glyph::split_marker;
use crate::shared::xmltree::{
    append_child, deep_clone, find_all_desc, find_child, find_children, find_desc, insert_element,
    ptr, remove_child, run_text, set_attr, text_nodes, Element, HP, NodeRef,
};
use crate::compile::patterns::MARKDOWN_BOLD_RE;
use crate::compile::template::{is_depth_kind, is_note_kind, SAMPLE_TEXT};

static BR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static BOLD_WRAP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\*\*(.*)\*\*$").unwrap());
static HEAD_NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+\.)\s*(.*)$").unwrap());
static LABEL_BODY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\([^)]*\))(.*)$").unwrap());
static SEP1_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\S(\s*)").unwrap());
static LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\([^()]*\)").unwrap());

/// run 안의 `hp:t`/`hp:lineBreak` 자식을 지우고 `<br>` 분할 텍스트를 채운다.
fn set_run_content(run: &NodeRef, text: &str) {
    let to_remove: Vec<NodeRef> = run
        .borrow()
        .children
        .iter()
        .filter(|c| {
            let b = c.borrow();
            b.is(HP, "t") || b.is(HP, "lineBreak")
        })
        .cloned()
        .collect();
    for c in to_remove {
        remove_child(run, &c);
    }
    let parts: Vec<&str> = BR_RE.split(text).collect();
    for (index, part) in parts.iter().enumerate() {
        let t = Rc::new(RefCell::new(Element::new("hp", "t", HP)));
        t.borrow_mut().set_text(part);
        append_child(run, t);
        if index < parts.len() - 1 {
            let lb = Rc::new(RefCell::new(Element::new("hp", "lineBreak", HP)));
            append_child(run, lb);
        }
    }
}

/// 문단/문자 스타일을 유지한 채 복제된 prototype에 텍스트를 채운다.
fn set_paragraph_text(paragraph: &NodeRef, text: &str) {
    let runs = find_children(paragraph, HP, "run");
    for r in &runs {
        set_run_content(r, "");
    }
    let run = if runs.is_empty() {
        let r = Rc::new(RefCell::new(Element::new("hp", "run", HP)));
        append_child(paragraph, r.clone());
        r
    } else {
        runs[0].clone()
    };
    set_run_content(&run, text);
}

pub fn replace_run_text(run: &NodeRef, text: &str, char_pr: Option<&str>) {
    if let Some(cp) = char_pr {
        set_attr(run, "charPrIDRef", cp);
    }
    set_run_content(run, text);
}

fn leading_whitespace(text: &str) -> String {
    text.chars().take_while(|c| c.is_whitespace()).collect()
}

fn first_t_text(run: &NodeRef) -> String {
    run.borrow()
        .children
        .iter()
        .find(|c| c.borrow().is(HP, "t"))
        .map(|c| c.borrow().text_or_empty())
        .unwrap_or_default()
}

/// 기존 텍스트의 선행 공백을 보존하면서 내용을 교체한다.
fn replace_run_text_preserving_leading(run: &NodeRef, text: &str, char_pr: Option<&str>) {
    let original = first_t_text(run);
    let combined = format!("{}{}", leading_whitespace(&original), text);
    replace_run_text(run, &combined, char_pr);
}

/// 구분 run을 지우되 템플릿 공백은 남긴다.
fn clear_run_preserving_leading(run: &NodeRef) {
    let original = first_t_text(run);
    let attr = run.borrow().get_attr("charPrIDRef");
    replace_run_text(run, &leading_whitespace(&original), attr.as_deref());
}

/// ensure_run_count로 새로 넣은 run에 템플릿 공백을 채운다.
fn fill_separator(run: &NodeRef, separator: &str, char_pr: Option<&str>) {
    if !separator.is_empty() && run_text(run).is_empty() {
        replace_run_text(run, separator, char_pr);
    }
}

/// 템플릿 run 하나 안에 글머리/라벨/본문 사이 공백으로 보존된 간격.
fn template_separators(text: &str) -> (String, String) {
    let c = match SEP1_RE.captures(text) {
        Some(c) => c,
        None => return (String::new(), String::new()),
    };
    let sep1 = c[1].to_string();
    let rest = &text[c.get(0).unwrap().end()..];
    let label_end = match LABEL_RE.captures(rest) {
        Some(l) => l.get(0).unwrap().end(),
        None => return (sep1, String::new()),
    };
    let trailing_ws: String = rest[label_end..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    (sep1, trailing_ws)
}

fn ensure_run_count(paragraph: &NodeRef, count: usize) -> Vec<NodeRef> {
    let mut runs = find_children(paragraph, HP, "run");
    while runs.len() < count {
        let run = Rc::new(RefCell::new(Element::new("hp", "run", HP)));
        let pos = {
            let pb = paragraph.borrow();
            pb.children
                .iter()
                .position(|c| c.borrow().is(HP, "linesegarray"))
                .unwrap_or(pb.children.len())
        };
        insert_element(paragraph, pos, run.clone());
        runs.push(run);
    }
    runs
}

/// 템플릿의 옛 짧은 텍스트에 캐시된 줄 배치를 제거한다.
fn invalidate_layout_cache(paragraph: &NodeRef) {
    let to_remove: Vec<NodeRef> = find_children(paragraph, HP, "linesegarray");
    for node in to_remove {
        remove_child(paragraph, &node);
    }
}

/// `**굵게**` 표식만 제거한다.
fn bold_sub(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0usize;
    for c in MARKDOWN_BOLD_RE.captures_iter(s) {
        let m = c.get(0).unwrap();
        out.push_str(&s[last..m.start()]);
        out.push_str(&c[1]);
        last = m.end();
    }
    out.push_str(&s[last..]);
    out
}

fn heading(paragraph: &NodeRef, text: &str) -> Result<()> {
    let source_text = text.trim();
    let target = match BOLD_WRAP_RE.captures(source_text) {
        Some(c) => c[1].to_string(),
        None => source_text.to_string(),
    };
    let (number, body) = match HEAD_NUM_RE.captures(&target) {
        Some(c) => (c[1].to_string(), c[2].to_string()),
        None => (String::new(), target.clone()),
    };
    let template_text: String = text_nodes(paragraph)
        .iter()
        .map(|n| n.borrow().text_or_empty())
        .collect();
    let rendered = template_text
        .replacen("#.", &number, 1)
        .replacen(SAMPLE_TEXT, &body, 1);
    set_paragraph_text(paragraph, &bold_sub(&rendered));
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// 2.1 형식 소제목을 heading:sub prototype에 렌더링한다. 번호(`2.1`)는
/// 본문의 일부로 보존한다 — 역변환(hwpx2md)과 재흡수(NUM_SUB_RE)가 번호를
/// 필요로 하며, 소스에 명시된 번호가 결과물에서 사라지는 것은 충실도 손실이다.
fn heading_sub(paragraph: &NodeRef, text: &str) -> Result<()> {
    let rendered = bold_sub(text.trim());
    match find_desc(paragraph, HP, "tbl") {
        None => set_paragraph_text(paragraph, &rendered),
        Some(table) => {
            let nodes = text_nodes(&table);
            let target = nodes
                .iter()
                .find(|n| n.borrow().text_or_empty().contains(SAMPLE_TEXT))
                .cloned()
                .ok_or_else(|| {
                    AppError("no sample text in the [[heading:sub]] table of template.hwpx.".into())
                })?;
            let mut tb = target.borrow_mut();
            let new_text = tb.text_or_empty().replacen("샘플", &rendered, 1);
            tb.set_text(&new_text);
        }
    }
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// 글머리/괄호 라벨/본문을 다섯 run으로 나눠 렌더링한다.
fn render_labeled_item(
    paragraph: &NodeRef,
    body: &str,
    marker: &str,
    char_pr: &HashMap<String, String>,
) -> Result<()> {
    let original_count = find_children(paragraph, HP, "run").len();
    let runs = ensure_run_count(paragraph, 5);
    let (sep1, sep2) = template_separators(&run_text(&runs[0]));
    let (label, rest) = match LABEL_BODY_RE.captures(body) {
        Some(c) => (c[1].to_string(), c[2].trim_start().to_string()),
        None => (String::new(), body.to_string()),
    };
    replace_run_text_preserving_leading(&runs[0], marker, Some(&char_pr["bullet"]));
    clear_run_preserving_leading(&runs[1]);
    if original_count <= 1 {
        fill_separator(&runs[1], &sep1, Some(&char_pr["bullet"]));
    }
    replace_run_text_preserving_leading(&runs[2], &label, Some(&char_pr["keyword"]));
    clear_run_preserving_leading(&runs[3]);
    if original_count <= 3 {
        fill_separator(&runs[3], &sep2, Some(&char_pr["keyword"]));
    }
    replace_run_text_preserving_leading(&runs[4], &rest, Some(&char_pr["body"]));
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// 주석 기호는 빨간색으로, 본문은 검정으로 유지한다.
fn set_note_item(
    paragraph: &NodeRef,
    body: &str,
    char_pr: &HashMap<String, String>,
) -> Result<()> {
    let original_count = find_children(paragraph, HP, "run").len();
    let runs = ensure_run_count(paragraph, 3);
    let (sep1, _) = template_separators(&run_text(&runs[0]));
    replace_run_text_preserving_leading(
        &runs[0],
        &char_pr["bullet_text"],
        Some(&char_pr["bullet"]),
    );
    clear_run_preserving_leading(&runs[1]);
    if original_count <= 1 {
        fill_separator(&runs[1], &sep1, Some(&char_pr["bullet"]));
    }
    replace_run_text_preserving_leading(&runs[2], body, Some(&char_pr["body"]));
    invalidate_layout_cache(paragraph);
    Ok(())
}

fn render_single_run(paragraph: &NodeRef, text: &str) -> Result<()> {
    if text.is_empty() {
        return err("nothing to put into the single-paragraph prototype.");
    }
    let nodes = text_nodes(paragraph);
    let target = nodes
        .iter()
        .rev()
        .find(|n| !n.borrow().text_or_empty().trim().is_empty())
        .cloned()
        .or_else(|| nodes.last().cloned());
    match target {
        None => set_paragraph_text(paragraph, text),
        Some(target) => {
            for node in &nodes {
                if Rc::ptr_eq(node, &target) {
                    let mut nb = node.borrow_mut();
                    let lead = leading_whitespace(&nb.text_or_empty());
                    nb.set_text(&format!("{lead}{text}"));
                } else if !node.borrow().text_or_empty().trim().is_empty() {
                    node.borrow_mut().set_text("");
                }
            }
        }
    }
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// [[box]] 글상자 표에 제목과 불릿 항목을 채운다. 제목은 표의 첫 "샘플"
/// 자리에 들어가고, 항목은 불릿 글자 자리와 본문 "샘플" 자리로 나뉜 셀
/// 문단이 항목 수만큼 복제돼 들어간다.
fn render_box(
    paragraph: &NodeRef,
    encoded_rows: &str,
    char_pr: &HashMap<String, String>,
) -> Result<()> {
    let rows: Vec<Vec<String>> = serde_json::from_str(encoded_rows)
        .map_err(|_| AppError("cannot parse box data.".into()))?;
    let title = rows
        .first()
        .and_then(|r| r.first())
        .cloned()
        .unwrap_or_default();
    let items: Vec<String> = rows
        .iter()
        .skip(1)
        .filter_map(|r| r.first().cloned())
        .collect();
    let table = find_desc(paragraph, HP, "tbl")
        .ok_or_else(|| AppError("box prototype table not found in template.hwpx.".into()))?;
    let bullet_char = char_pr["bullet_text"].clone();
    let (title_node, content_paragraph) = locate_box_samples(&table)?;

    {
        let mut tb = title_node.borrow_mut();
        let replaced = tb.text_or_empty().replacen(SAMPLE_TEXT, &title, 1);
        tb.set_text(&replaced);
    }
    let pristine = deep_clone(&content_paragraph);
    let rendered: Vec<(String, String)> = items
        .iter()
        .map(|item| split_marker(item, &bullet_char))
        .collect();
    let (first_marker, first_body) = rendered
        .first()
        .map_or(("", ""), |(m, b)| (m.as_str(), b.as_str()));
    fill_box_item(&content_paragraph, first_marker, first_body, &bullet_char);
    if rendered.len() > 1 {
        let sub_list = find_all_desc(&table, HP, "subList").into_iter().find(|sl| {
            let b = sl.borrow();
            b.children.iter().any(|c| ptr(c) == ptr(&content_paragraph))
        });
        let Some(sub_list) = sub_list else {
            return err("item cell subList not found in the [[box]] table of template.hwpx.");
        };
        let mut anchor = ptr(&content_paragraph);
        for (marker, body) in &rendered[1..] {
            let clone = deep_clone(&pristine);
            fill_box_item(&clone, marker, body, &bullet_char);
            let pos = {
                let b = sub_list.borrow();
                b.children
                    .iter()
                    .position(|c| ptr(c) == anchor)
                    .map(|p| p + 1)
                    .unwrap_or(b.children.len())
            };
            insert_element(&sub_list, pos, clone.clone());
            anchor = ptr(&clone);
        }
    }
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// 박스 표에서 제목 "샘플" 자리와 그 자리가 속한 항목 문단을 찾는다.
fn locate_box_samples(table: &NodeRef) -> Result<(NodeRef, NodeRef)> {
    let mut samples = text_nodes(table)
        .into_iter()
        .filter(|n| n.borrow().text_or_empty().contains(SAMPLE_TEXT));
    let title_node = samples
        .next()
        .ok_or_else(|| AppError("no title sample text in the [[box]] table of template.hwpx.".into()))?;
    let body_node = samples
        .next()
        .ok_or_else(|| AppError("no item sample text in the [[box]] table of template.hwpx.".into()))?;
    let content_paragraph = find_all_desc(table, HP, "p")
        .into_iter()
        .find(|p| text_nodes(p).iter().any(|n| ptr(n) == ptr(&body_node)))
        .ok_or_else(|| AppError("item paragraph not found in the [[box]] table of template.hwpx.".into()))?;
    Ok((title_node, content_paragraph))
}

/// 박스 항목 문단 하나에 출력 불릿과 본문을 채운다.
fn fill_box_item(paragraph: &NodeRef, marker: &str, body: &str, bullet_char: &str) {
    for node in text_nodes(paragraph) {
        let text = node.borrow().text_or_empty();
        if text.contains(bullet_char) {
            let lead = leading_whitespace(&text);
            node.borrow_mut().set_text(&format!("{lead}{marker}"));
        } else if text.contains(SAMPLE_TEXT) {
            node.borrow_mut().set_text(body);
        }
    }
    invalidate_layout_cache(paragraph);
}

/// [[highlight]] 블록을 highlight prototype 표에 렌더링한다. 항목 하나당 셀
/// 문단 하나를 채워 넣어 항목 경계를 보존한다 — 역변환(hwpx2md)이 문단
/// 경계를 항목으로 복원하므로 공백으로 합치는 것보다 왕복 손실이 없다.
fn render_highlight(paragraph: &NodeRef, encoded: &str) -> Result<()> {
    let items: Vec<String> = serde_json::from_str(encoded)
        .map_err(|_| AppError("cannot parse highlight data.".into()))?;
    if items.is_empty() {
        return err("highlight block has no content.");
    }
    let Some(table) = find_desc(paragraph, HP, "tbl") else {
        // 표 프로토타입이 아니면 종전처럼 한 문장으로 합쳐 넣는다.
        return render_single_run(paragraph, &items.join(" "));
    };
    let sample_paragraph = find_all_desc(&table, HP, "p")
        .into_iter()
        .find(|p| {
            text_nodes(p)
                .iter()
                .any(|n| n.borrow().text_or_empty().contains(SAMPLE_TEXT))
        })
        .ok_or_else(|| {
            AppError("no sample text in the [[highlight]] table of template.hwpx.".into())
        })?;
    let pristine = deep_clone(&sample_paragraph);
    fill_highlight_item(&sample_paragraph, &items[0]);
    if items.len() > 1 {
        let sub_list = find_all_desc(&table, HP, "subList").into_iter().find(|sl| {
            let b = sl.borrow();
            b.children.iter().any(|c| ptr(c) == ptr(&sample_paragraph))
        });
        let Some(sub_list) = sub_list else {
            return err(
                "sample cell subList not found in the [[highlight]] table of template.hwpx.",
            );
        };
        let mut anchor = ptr(&sample_paragraph);
        for item in &items[1..] {
            let clone = deep_clone(&pristine);
            fill_highlight_item(&clone, item);
            let pos = {
                let b = sub_list.borrow();
                b.children
                    .iter()
                    .position(|c| ptr(c) == anchor)
                    .map(|p| p + 1)
                    .unwrap_or(b.children.len())
            };
            insert_element(&sub_list, pos, clone.clone());
            anchor = ptr(&clone);
        }
    }
    invalidate_layout_cache(paragraph);
    Ok(())
}

fn fill_highlight_item(paragraph: &NodeRef, item: &str) {
    let nodes = text_nodes(paragraph);
    let target = nodes
        .iter()
        .rev()
        .find(|n| n.borrow().text_or_empty().contains(SAMPLE_TEXT))
        .or_else(|| {
            nodes
                .iter()
                .rev()
                .find(|n| !n.borrow().text_or_empty().trim().is_empty())
        })
        .cloned();
    for node in &nodes {
        if let Some(t) = &target {
            if Rc::ptr_eq(node, t) {
                let lead = leading_whitespace(&node.borrow().text_or_empty());
                node.borrow_mut().set_text(&format!("{lead}{item}"));
                continue;
            }
        }
        if !node.borrow().text_or_empty().trim().is_empty() {
            node.borrow_mut().set_text("");
        }
    }
    invalidate_layout_cache(paragraph);
}

fn optimize_column_widths(widths: &[i64], rows: &[Vec<String>]) -> Vec<i64> {
    let total_width: i64 = widths.iter().sum();
    let column_count = widths.len();

    fn display_width(text: &str) -> i64 {
        text.chars()
            .map(|c| {
                if (c as u32) >= 0x3000 || ('가'..='힣').contains(&c) {
                    2
                } else {
                    1
                }
            })
            .sum()
    }

    let content_widths: Vec<i64> = (0..column_count)
        .map(|i| {
            rows.iter()
                .map(|r| display_width(&r[i]))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut minimum = std::cmp::max(4000, total_width / (column_count as i64 * 2));
    if minimum * column_count as i64 > total_width {
        minimum = std::cmp::max(1, total_width / column_count as i64);
    }
    let remaining = std::cmp::max(0, total_width - minimum * column_count as i64);
    let weight_total = {
        let s: i64 = content_widths.iter().sum();
        if s == 0 {
            column_count as i64
        } else {
            s
        }
    };
    let mut optimized: Vec<i64> = content_widths
        .iter()
        .map(|&w| minimum + remaining * w / weight_total)
        .collect();
    let sum: i64 = optimized.iter().sum();
    let last = optimized.len() - 1;
    optimized[last] += total_width - sum;
    optimized
}

/// prototype 셀 너비를 검증하고 입력 열 수에 맞춰 재분배한다.
fn prepare_column_widths(
    template_widths: Vec<i64>,
    column_count: usize,
    rows: &[Vec<String>],
) -> Result<Vec<i64>> {
    if template_widths.is_empty() || template_widths.contains(&0) {
        return err("cannot read cell width of the table prototype.");
    }
    let mut column_widths: Vec<i64> = if column_count > template_widths.len() {
        // 모자란 열은 마지막 셀 너비를 나눠 채운다.
        let tail_width = *template_widths.last().unwrap();
        let split_count = (column_count - template_widths.len() + 1) as i64;
        let shared = tail_width.div_euclid(split_count);
        let remainder = tail_width.rem_euclid(split_count);
        let mut cw = template_widths[..template_widths.len() - 1].to_vec();
        cw.extend(std::iter::repeat_n(shared, split_count as usize));
        let last = cw.len() - 1;
        cw[last] += remainder;
        cw
    } else {
        template_widths[..column_count].to_vec()
    };
    column_widths = optimize_column_widths(&column_widths, rows);
    Ok(column_widths)
}

/// 셀 하나에 주소/너비/본문을 채우고 남은 템플릿 문단은 비운다.
fn fill_cell(
    cell: &NodeRef,
    column_index: usize,
    row_index: usize,
    value: &str,
    width: i64,
) -> Result<()> {
    set_attr(cell, "name", "");
    if let Some(addr) = find_child(cell, HP, "cellAddr") {
        set_attr(&addr, "colAddr", &column_index.to_string());
        set_attr(&addr, "rowAddr", &row_index.to_string());
    }
    if let Some(sz) = find_child(cell, HP, "cellSz") {
        set_attr(&sz, "width", &width.to_string());
    }
    let cell_paragraph = find_desc(cell, HP, "p")
        .ok_or_else(|| AppError("no paragraph in the table prototype cell.".into()))?;
    set_paragraph_text(&cell_paragraph, value);
    invalidate_layout_cache(&cell_paragraph);
    for extra in find_all_desc(cell, HP, "p").into_iter().skip(1) {
        set_paragraph_text(&extra, "");
        invalidate_layout_cache(&extra);
    }
    Ok(())
}

/// 표 prototype을 입력 행·열 수에 맞게 확장/축소한다.
fn render_table(paragraph: &NodeRef, encoded_rows: &str) -> Result<()> {
    let rows: Vec<Vec<String>> = serde_json::from_str(encoded_rows)
        .map_err(|_| AppError("cannot parse Markdown table data.".into()))?;
    if rows.is_empty()
        || rows.iter().any(|r| r.is_empty())
        || rows.iter().map(|r| r.len()).collect::<HashSet<_>>().len() != 1
    {
        return err("invalid row/column count in the Markdown table.");
    }
    let table = find_desc(paragraph, HP, "tbl")
        .ok_or_else(|| AppError("table prototype not found in template.hwpx.".into()))?;
    let template_rows = find_children(&table, HP, "tr");
    if template_rows.is_empty() {
        return err("table prototype has no rows.");
    }
    let header_template = deep_clone(&template_rows[0]);
    let body_template = deep_clone(template_rows.get(1).unwrap_or(&template_rows[0]));
    let template_cells = find_children(&template_rows[0], HP, "tc");
    let template_widths: Vec<i64> = template_cells
        .iter()
        .map(|cell| {
            find_child(cell, HP, "cellSz")
                .and_then(|sz| sz.borrow().get_attr("width"))
                .and_then(|w| w.parse::<i64>().ok())
                .unwrap_or(0)
        })
        .collect();
    let column_count = rows[0].len();
    let column_widths = prepare_column_widths(template_widths, column_count, &rows)?;

    for row in &template_rows {
        remove_child(&table, row);
    }
    for (row_index, values) in rows.iter().enumerate() {
        let row = if row_index == 0 {
            deep_clone(&header_template)
        } else {
            deep_clone(&body_template)
        };
        let mut cells = find_children(&row, HP, "tc");
        while cells.len() < column_count {
            let cell = deep_clone(cells.last().unwrap());
            append_child(&row, cell.clone());
            cells.push(cell);
        }
        for cell in &cells[column_count..] {
            remove_child(&row, cell);
        }
        for (column_index, value) in values.iter().enumerate() {
            fill_cell(&cells[column_index], column_index, row_index, value, column_widths[column_index])?;
        }
        append_child(&table, row);
    }
    set_attr(&table, "rowCnt", &rows.len().to_string());
    set_attr(&table, "colCnt", &column_count.to_string());
    invalidate_layout_cache(paragraph);
    Ok(())
}

/// 하나의 의미 블록을 대응 prototype 렌더러로 보낸다.
pub fn render_block(
    kind: &str,
    paragraph: &NodeRef,
    value: &str,
    styles: &HashMap<String, HashMap<String, String>>,
) -> Result<()> {
    if kind == "heading_default" {
        return heading(paragraph, value);
    }
    if kind == "heading_sub" {
        return heading_sub(paragraph, value);
    }
    if is_depth_kind(kind) {
        let style = &styles[kind];
        let (marker, body) = split_marker(value, &style["bullet_text"]);
        return render_labeled_item(paragraph, &body, &marker, style);
    }
    if is_note_kind(kind) {
        return set_note_item(paragraph, value, &styles[kind]);
    }
    if kind == "box" {
        return render_box(paragraph, value, &styles[kind]);
    }
    if kind == "highlight" {
        return render_highlight(paragraph, value);
    }
    if kind == "table" {
        return render_table(paragraph, value);
    }
    err(format!("unsupported block kind: {kind}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::xmltree::parse;

    fn box_fixture() -> NodeRef {
        // 템플릿의 박스 표와 같은 모양: 제목 셀(첫 "샘플") + 내용 셀(불릿과
        // 본문 "샘플"이 같은 문단에 fwSpace로 나뉘어 있음).
        let xml = r#"<hp:p xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:run charPrIDRef="1"><hp:tbl><hp:tr><hp:tc><hp:subList><hp:p><hp:run charPrIDRef="39"><hp:t>&lt; </hp:t><hp:t>샘플</hp:t><hp:t> &gt;</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr><hp:tr><hp:tc><hp:subList><hp:p><hp:run charPrIDRef="41"><hp:t><hp:fwSpace/>∙</hp:t><hp:t><hp:fwSpace/></hp:t><hp:t>샘플</hp:t></hp:run><hp:run charPrIDRef="40"><hp:t/></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#;
        parse(xml.as_bytes()).unwrap()
    }

    fn highlight_fixture() -> NodeRef {
        // 템플릿의 highlight 표와 같은 모양: 1×1 표, 셀 문단은 선행 공백 +
        // "샘플" 한 런.
        let xml = r#"<hp:p xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:run charPrIDRef="1"><hp:tbl><hp:tr><hp:tc><hp:subList><hp:p><hp:run charPrIDRef="54"><hp:t> 샘플</hp:t></hp:run></hp:p></hp:subList></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#;
        parse(xml.as_bytes()).unwrap()
    }

    #[test]
    fn render_highlight_writes_one_paragraph_per_item() {
        let paragraph = highlight_fixture();
        render_highlight(&paragraph, r#"["첫 항목","둘째 항목"]"#).unwrap();

        let table = find_desc(&paragraph, HP, "tbl").unwrap();
        let texts: Vec<String> = find_all_desc(&table, HP, "p")
            .iter()
            .map(|p| {
                text_nodes(p)
                    .iter()
                    .map(|n| n.borrow().text_or_empty())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        // 항목 하나당 셀 문단 하나 — 선행 공백(마커 자리)은 보존된다.
        assert_eq!(texts, vec![" 첫 항목", " 둘째 항목"], "texts: {texts:?}");
    }

    #[test]
    fn render_highlight_single_item_keeps_one_paragraph() {
        let paragraph = highlight_fixture();
        render_highlight(&paragraph, r#"["핵심 요약 문장"]"#).unwrap();
        let table = find_desc(&paragraph, HP, "tbl").unwrap();
        let texts: Vec<String> = text_nodes(&table)
            .iter()
            .map(|n| n.borrow().text_or_empty())
            .collect();
        assert!(texts.contains(&" 핵심 요약 문장".to_string()), "texts: {texts:?}");
    }

    fn styles() -> HashMap<String, String> {
        HashMap::from([
            ("title".to_string(), "39".to_string()),
            ("bullet".to_string(), "41".to_string()),
            ("body".to_string(), "41".to_string()),
            ("bullet_text".to_string(), "∙".to_string()),
        ])
    }

    #[test]
    fn render_box_fills_title_and_duplicates_item_paragraphs() {
        let paragraph = box_fixture();
        let encoded = r#"[["요약 제목"],["첫 항목"],["① 둘째 항목"]]"#;
        render_box(&paragraph, encoded, &styles()).unwrap();

        let table = find_desc(&paragraph, HP, "tbl").unwrap();
        let texts: Vec<String> = text_nodes(&table)
            .iter()
            .map(|n| n.borrow().text_or_empty())
            .collect();
        assert!(texts.contains(&"요약 제목".to_string()), "texts: {texts:?}");
        // 제목 셀의 장식 run("< ", " >")은 그대로 남는다.
        assert_eq!(texts[0], "< ");
        assert_eq!(texts[2], " >");
        // 항목 하나당 내용 셀 문단이 하나씩 생긴다.
        let content_texts: Vec<Vec<String>> = find_all_desc(&table, HP, "p")
            .iter()
            .map(|p| {
                text_nodes(p)
                    .iter()
                    .map(|n| n.borrow().text_or_empty())
                    .collect()
            })
            .filter(|ts: &Vec<String>| {
                ts.iter().any(|t| !t.trim().is_empty()) && ts.len() >= 2
            })
            .collect();
        assert_eq!(content_texts.len(), 3, "content paragraphs: {content_texts:?}");
        assert_eq!(
            content_texts[1],
            vec![" ∙".to_string(), " ".to_string(), "첫 항목".to_string(), String::new()]
        );
        // 원문자 번호는 출력 불릿 자리에, 본문은 그 뒤에 남는다.
        assert_eq!(content_texts[2][0], " ①");
        assert_eq!(content_texts[2][2], "둘째 항목");
    }

    #[test]
    fn render_box_without_items_clears_sample() {
        let paragraph = box_fixture();
        render_box(&paragraph, r#"[["제목만"]]"#, &styles()).unwrap();
        let table = find_desc(&paragraph, HP, "tbl").unwrap();
        let texts: Vec<String> = text_nodes(&table)
            .iter()
            .map(|n| n.borrow().text_or_empty())
            .collect();
        assert!(texts.contains(&"제목만".to_string()), "texts: {texts:?}");
        assert!(!texts.iter().any(|t| t.contains("샘플")), "texts: {texts:?}");
    }
}
