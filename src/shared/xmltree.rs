use std::cell::RefCell;
use std::rc::Rc;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::error::{err, AppError, Result};

/// 재귀 순회(deep_clone/iter/serialize)와 Rc 드롭 연쇄가 스택을 다 쓰지
/// 못하게 하는 중첩 상한. 정상 HWPX 문단 트리보다 훨씬 깊은 값이다.
const MAX_XML_DEPTH: usize = 512;

/// HWPX 표준 네임스페이스 URI.
pub const HP: &str = "http://www.hancom.co.kr/hwpml/2011/paragraph";
pub const HH: &str = "http://www.hancom.co.kr/hwpml/2011/head";

/// 공유 가변 요소 노드. Python ElementTree와 같은 모델(요소의 자식은
/// 요소뿐이고 텍스트는 .text/.tail 속성)로 만들어 hwp.py 로직을 그대로
/// 옮길 수 있게 한다. `Rc`로 요소 동일성 비교(`is`)를 흉내 낸다.
pub type NodeRef = Rc<RefCell<Element>>;

#[derive(Clone, Debug)]
pub struct Element {
    pub prefix: Option<String>,
    pub local: String,
    /// 태그가 속한 네임스페이스 URI. 접두사가 아니라 URI로 비교한다.
    pub ns: Option<String>,
    /// 원본 속성 (xmlns:* 선언 포함, 문서 순서 유지).
    pub attrs: Vec<(String, String)>,
    pub text: Option<String>,
    pub tail: Option<String>,
    pub children: Vec<NodeRef>,
}

impl Element {
    pub fn new(prefix: &str, local: &str, ns: &str) -> Element {
        Element {
            prefix: Some(prefix.to_string()),
            local: local.to_string(),
            ns: Some(ns.to_string()),
            attrs: Vec::new(),
            text: None,
            tail: None,
            children: Vec::new(),
        }
    }

    pub fn is(&self, uri: &str, local: &str) -> bool {
        // 네임스페이스 선언이 없는 요소(ns: None)는 빈 URI와 같게 취급한다.
        let own_ns = self.ns.as_deref().unwrap_or("");
        own_ns == uri && self.local == local
    }

    pub fn get_attr(&self, name: &str) -> Option<String> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        for (k, v) in self.attrs.iter_mut() {
            if k == name {
                *v = value.to_string();
                return;
            }
        }
        self.attrs.push((name.to_string(), value.to_string()));
    }

    pub fn remove_attr(&mut self, name: &str) -> Option<String> {
        self.attrs
            .iter()
            .position(|(k, _)| k == name)
            .map(|pos| self.attrs.remove(pos).1)
    }

    pub fn text_or_empty(&self) -> String {
        self.text.clone().unwrap_or_default()
    }

    pub fn set_text(&mut self, value: &str) {
        self.text = Some(value.to_string());
    }
}

/// Python `id(element)`에 해당하는 동일성 키.
pub fn ptr(r: &NodeRef) -> usize {
    Rc::as_ptr(r) as *const Element as usize
}

pub fn deep_clone(r: &NodeRef) -> NodeRef {
    let b = r.borrow();
    Rc::new(RefCell::new(Element {
        prefix: b.prefix.clone(),
        local: b.local.clone(),
        ns: b.ns.clone(),
        attrs: b.attrs.clone(),
        text: b.text.clone(),
        tail: b.tail.clone(),
        children: b.children.iter().map(deep_clone).collect(),
    }))
}

/// `element.iter()`과 같은 순서(자기 자신 포함, 문서 순서)의 요소 순회.
pub fn iter_elements(root: &NodeRef) -> Vec<NodeRef> {
    fn walk(r: &NodeRef, out: &mut Vec<NodeRef>) {
        out.push(r.clone());
        let child_count = r.borrow().children.len();
        for i in 0..child_count {
            let child = r.borrow().children[i].clone();
            walk(&child, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// `element.iter("{uri}tag")`에 해당.
pub fn iter_tag(root: &NodeRef, uri: &str, local: &str) -> Vec<NodeRef> {
    iter_elements(root)
        .into_iter()
        .filter(|n| n.borrow().is(uri, local))
        .collect()
}

/// `element.findall(".//{uri}tag")` — 자기 자신 제외한 모든 하위 요소.
pub fn find_all_desc(root: &NodeRef, uri: &str, local: &str) -> Vec<NodeRef> {
    iter_elements(root)
        .into_iter()
        .skip(1)
        .filter(|n| n.borrow().is(uri, local))
        .collect()
}

/// `element.find(".//{uri}tag")` — 문서 순서 첫 번째 하위 요소.
pub fn find_desc(root: &NodeRef, uri: &str, local: &str) -> Option<NodeRef> {
    find_all_desc(root, uri, local).into_iter().next()
}

/// `element.findall("{uri}tag")` — 직계 자식만.
pub fn find_children(parent: &NodeRef, uri: &str, local: &str) -> Vec<NodeRef> {
    parent
        .borrow()
        .children
        .iter()
        .filter(|c| c.borrow().is(uri, local))
        .cloned()
        .collect()
}

/// `element.find("{uri}tag")` — 첫 번째 직계 자식.
pub fn find_child(parent: &NodeRef, uri: &str, local: &str) -> Option<NodeRef> {
    find_children(parent, uri, local).into_iter().next()
}

/// `list(parent).index(child)` — 직계 자식 중 위치.
pub fn element_index(parent: &NodeRef, child: &NodeRef) -> Option<usize> {
    parent
        .borrow()
        .children
        .iter()
        .position(|c| Rc::ptr_eq(c, child))
}

pub fn remove_child(parent: &NodeRef, child: &NodeRef) -> bool {
    let mut pb = parent.borrow_mut();
    match pb.children.iter().position(|c| Rc::ptr_eq(c, child)) {
        Some(pos) => {
            pb.children.remove(pos);
            true
        }
        None => false,
    }
}

/// `parent.insert(index, child)` — index는 요소 자식 기준(텍스트는
/// 자식이 아니므로 ET와 동일).
pub fn insert_element(parent: &NodeRef, index: usize, child: NodeRef) {
    let mut pb = parent.borrow_mut();
    let pos = index.min(pb.children.len());
    pb.children.insert(pos, child);
}

pub fn append_child(parent: &NodeRef, child: NodeRef) {
    parent.borrow_mut().children.push(child);
}

/// run의 직계 `hp:t` 자식 텍스트를 이어 붙인다.
pub fn run_text(run: &NodeRef) -> String {
    run.borrow()
        .children
        .iter()
        .filter(|c| c.borrow().is(HP, "t"))
        .map(|c| c.borrow().text_or_empty())
        .collect()
}

/// `element.findall(".//{hp}t")`.
pub fn text_nodes(root: &NodeRef) -> Vec<NodeRef> {
    find_all_desc(root, HP, "t")
}

pub fn set_attr(node: &NodeRef, name: &str, value: &str) {
    node.borrow_mut().set_attr(name, value);
}

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#09;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

fn push_tag_name(b: &Element, out: &mut String) {
    if let Some(p) = &b.prefix {
        out.push_str(p);
        out.push(':');
    }
    out.push_str(&b.local);
}

fn serialize_element(b: &Element, out: &mut String) {
    out.push('<');
    push_tag_name(b, out);
    for (k, v) in &b.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&escape_attr(v));
        out.push('"');
    }
    let has_text = b.text.as_deref().is_some_and(|t| !t.is_empty());
    if b.children.is_empty() && !has_text {
        out.push_str(" />");
        return;
    }
    out.push('>');
    if has_text {
        out.push_str(&escape_text(b.text.as_deref().unwrap()));
    }
    for c in &b.children {
        let cb = c.borrow();
        serialize_element(&cb, out);
        if let Some(t) = &cb.tail {
            if !t.is_empty() {
                out.push_str(&escape_text(t));
            }
        }
    }
    out.push_str("</");
    push_tag_name(b, out);
    out.push('>');
}

/// `ET.tostring(root, encoding="utf-8", xml_declaration=True)`에 해당.
pub fn serialize(root: &NodeRef, xml_decl: bool) -> String {
    let mut out = String::new();
    if xml_decl {
        out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>");
    }
    let b = root.borrow();
    serialize_element(&b, &mut out);
    out
}

/// tail 없이 요소 하나만 직렬화 — charPr 중복 검사용 시그니처.
pub fn serialize_signature(root: &NodeRef) -> Vec<u8> {
    let mut out = String::new();
    serialize_element(&root.borrow(), &mut out);
    out.into_bytes()
}

fn split_qname(name: &[u8]) -> (Option<&[u8]>, &[u8]) {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => (Some(&name[..pos]), &name[pos + 1..]),
        None => (None, name),
    }
}

/// 접두사를 네임스페이스 URI로 해석. 자기 요소의 선언이 가장 안쪽이고,
/// 그다음은 조상 선언 순서대로 적용한다.
fn resolve_ns(
    prefix: Option<&[u8]>,
    own: &[(String, String)],
    stack: &[Vec<(String, String)>],
) -> Option<String> {
    let key = prefix
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .unwrap_or_default();
    for (pfx, uri) in own {
        if *pfx == key {
            return Some(uri.clone());
        }
    }
    for frame in stack.iter().rev() {
        for (pfx, uri) in frame {
            if *pfx == key {
                return Some(uri.clone());
            }
        }
    }
    None
}

fn build_element(
    e: &BytesStart,
    ns_stack: &[Vec<(String, String)>],
) -> Result<(Element, Vec<(String, String)>)> {
    let name = e.name().as_ref().to_vec();
    let (prefix, local) = split_qname(&name);
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut decls: Vec<(String, String)> = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|er| AppError(format!("failed to decode XML attribute: {er}")))?;
        let key = a.key.as_ref().to_vec();
        let value = a
            .unescape_value()
            .map_err(|er| AppError(format!("failed to decode XML attribute: {er}")))?
            .into_owned();
        let (apfx, alocal) = split_qname(&key);
        match apfx {
            Some(p) if p == b"xmlns" => {
                let bound = String::from_utf8_lossy(alocal).into_owned();
                decls.push((bound, value.clone()));
            }
            None if alocal == b"xmlns" => {
                decls.push((String::new(), value.clone()));
            }
            _ => {}
        }
        attrs.push((String::from_utf8_lossy(&key).into_owned(), value));
    }
    let ns = resolve_ns(prefix, &decls, ns_stack);
    Ok((
        Element {
            prefix: prefix.map(|p| String::from_utf8_lossy(p).into_owned()),
            local: String::from_utf8_lossy(local).into_owned(),
            ns,
            attrs,
            text: None,
            tail: None,
            children: Vec::new(),
        },
        decls,
    ))
}

fn append_text(stack: &mut [NodeRef], s: &str) {
    let Some(top) = stack.last() else { return };
    let mut tb = top.borrow_mut();
    if let Some(last) = tb.children.last() {
        let mut lb = last.borrow_mut();
        let merged = match &lb.tail {
            Some(t) => format!("{t}{s}"),
            None => s.to_string(),
        };
        lb.tail = Some(merged);
    } else {
        let merged = match &tb.text {
            Some(t) => format!("{t}{s}"),
            None => s.to_string(),
        };
        tb.text = Some(merged);
    }
}

/// XML 파싱. Python ElementTree처럼 주석·PI·DOCTYPE은 버리고,
/// xmlns 선언은 원본 그대로 속성에 보존한다(직렬화 때 원본 접두사 유지).
pub fn parse(data: &[u8]) -> Result<NodeRef> {
    let mut reader = Reader::from_reader(data);
    let mut ns_stack: Vec<Vec<(String, String)>> = Vec::new();
    let mut stack: Vec<NodeRef> = Vec::new();
    let mut root: Option<NodeRef> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| AppError(format!("XML parse failed: {e}")))?;
        match event {
            Event::Start(e) => {
                if stack.len() >= MAX_XML_DEPTH {
                    return err(format!(
                        "XML nesting too deep (limit {MAX_XML_DEPTH})."
                    ));
                }
                let (elem, decls) = build_element(&e, &ns_stack)?;
                ns_stack.push(decls);
                let node: NodeRef = Rc::new(RefCell::new(elem));
                match stack.last() {
                    Some(top) => top.borrow_mut().children.push(node.clone()),
                    None => {
                        if root.is_some() {
                            return err("XML has more than one root element.");
                        }
                        root = Some(node.clone());
                    }
                }
                stack.push(node);
            }
            Event::Empty(e) => {
                let (elem, _) = build_element(&e, &ns_stack)?;
                // hp:fwSpace(고정폭 공백)을 자식 요소로 두면 뒤따르는 텍스트가
                // tail로 분리되어 text_nodes 기반 유틸이 모두 못 본다. 파싱
                // 시점에 일반 공백으로 흡수해 텍스트를 하나로 유지한다.
                if elem.ns.as_deref() == Some(HP) && elem.local == "fwSpace" {
                    append_text(&mut stack, " ");
                    continue;
                }
                let node: NodeRef = Rc::new(RefCell::new(elem));
                match stack.last() {
                    Some(top) => top.borrow_mut().children.push(node),
                    None => {
                        if root.is_some() {
                            return err("XML has more than one root element.");
                        }
                        root = Some(node);
                    }
                }
            }
            Event::Text(t) => {
                let s = t
                    .unescape()
                    .map_err(|e| AppError(format!("failed to decode XML text: {e}")))?
                    .into_owned();
                append_text(&mut stack, &s);
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(t.into_inner().as_ref()).into_owned();
                append_text(&mut stack, &s);
            }
            Event::End(_) => {
                ns_stack.pop();
                stack.pop();
            }
            Event::Eof => {
                // 닫히지 않은 태그가 남으면 Python ET처럼 오류로 본다.
                if !stack.is_empty() {
                    return err("unclosed XML tag.");
                }
                break;
            }
            _ => {}
        }
    }
    root.ok_or_else(|| AppError("XML root element not found.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_namespaces_and_entities() {
        let data = r#"<hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p paraPrIDRef="1"><hp:run charPrIDRef="2"><hp:t>가&amp;b&lt;></hp:t></hp:run></hp:p></hp:hs>"#.as_bytes();
        let root = parse(data).unwrap();
        assert!(root.borrow().is("http://www.hancom.co.kr/hwpml/2011/paragraph", "hs"));
        let out = serialize(&root, false);
        assert_eq!(
            out,
            "<hp:hs xmlns:hp=\"http://www.hancom.co.kr/hwpml/2011/paragraph\">\
<hp:p paraPrIDRef=\"1\"><hp:run charPrIDRef=\"2\"><hp:t>가&amp;b&lt;&gt;</hp:t></hp:run></hp:p></hp:hs>"
        );
    }

    #[test]
    fn finds_by_namespace_uri_not_prefix() {
        let data = r#"<a:root xmlns:a="urn:x"><a:t>1</a:t><b:t xmlns:b="urn:x">2</b:t></a:root>"#.as_bytes();
        let root = parse(data).unwrap();
        assert_eq!(find_all_desc(&root, "urn:x", "t").len(), 2);
    }

    #[test]
    fn clone_is_independent() {
        let data = r#"<r><p><t>샘플</t></p></r>"#.as_bytes();
        let root = parse(data).unwrap();
        let p = find_desc(&root, "", "p").unwrap();
        let c = deep_clone(&p);
        c.borrow_mut().children[0].borrow_mut().set_text("바뀜");
        assert_eq!(p.borrow().children[0].borrow().text_or_empty(), "샘플");
    }

    #[test]
    fn rejects_unclosed_tags() {
        let data = b"<r><p><t>unclosed</t></p>";
        assert!(parse(data).is_err());
    }

    #[test]
    fn absorbs_fwspace_into_text() {
        // fwSpace를 자식으로 두면 텍스트가 tail로 분리되어 유틸이 못 본다.
        // 파싱 시점에 일반 공백으로 흡수해 hp:t의 text를 하나로 유지한다.
        let data = r#"<hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:t>’26.<hp:fwSpace/>8.<hp:fwSpace/>25(금), 부록<hp:fwSpace/>(☎021)</hp:t></hp:hs>"#.as_bytes();
        let root = parse(data).unwrap();
        let t = find_desc(&root, HP, "t").unwrap();
        assert_eq!(
            t.borrow().text_or_empty(),
            "’26. 8. 25(금), 부록 (☎021)"
        );
        assert!(find_all_desc(&root, HP, "fwSpace").is_empty());
    }

    #[test]
    fn rejects_excessively_nested_xml() {
        // 재귀 순회가 스택을 다 쓰기 전에 파싱 단계에서 거부한다.
        let mut data = Vec::new();
        for _ in 0..100_000 {
            data.extend_from_slice(b"<a>");
        }
        assert!(parse(&data).is_err());
    }
}
