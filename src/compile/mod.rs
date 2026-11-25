//! 마크다운 + template.hwpx 바이트 → HWPX 바이트 인메모리 변환 공개 API.
//! 파일 I/O는 없으며, CLI(main.rs)와 Wasm(web/wasm)이 함께 쓴다.

pub mod cli;
pub mod hwpx;
pub mod linefit;
pub mod metrics;
pub mod patterns;
pub mod postprocess;
pub mod render;
pub mod source;
pub mod template;

pub use crate::shared::{error, glyph, xmltree};


use std::collections::HashMap;

pub use error::{AppError, Result};
pub use hwpx::{Report, Validation};

/// 에디터 문자열을 바로 파싱한다(출처 라벨 "editor").
pub fn parse_source_str(text: &str) -> std::result::Result<(String, Vec<source::Block>), AppError> {
    source::parse_source_str(text)
}

/// Wasm/기본 진입점. 소스는 "editor", 템플릿은 "template" 라벨로 표시된다.
pub fn compile_hwpx_bytes(
    source_md: &str,
    template_bytes: &[u8],
    variants: &[(String, String)],
) -> Result<(Vec<u8>, Report)> {
    compile_hwpx_labeled(source_md, template_bytes, variants, "editor", "template")
}

/// CLI용 진입점. 오류 메시지에 실제 파일 경로를 표시한다.
pub fn compile_hwpx_labeled(
    source_md: &str,
    template_bytes: &[u8],
    variants: &[(String, String)],
    source_label: &str,
    template_label: &str,
) -> Result<(Vec<u8>, Report)> {
    let (title, mut blocks) = source::parse_source_str_labeled(source_md, source_label)?;
    let (root, header_root) = hwpx::read_template_parts_bytes(template_bytes, template_label)?;
    let mut requested: HashMap<String, String> = variants
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut title_override: Option<String> = None;
    let mut source_variant: Option<String> = None;
    blocks.retain(|(kind, value)| match kind.as_str() {
        source::TITLE_VARIANT_BLOCK => {
            source_variant = Some(value.clone());
            false
        }
        source::TITLE_TEXT_BLOCK => {
            title_override = Some(value.clone());
            false
        }
        _ => true,
    });
    if let Some(name) = &source_variant {
        requested.insert("title".to_string(), name.clone());
    }
    let root = template::build_section(
        &root,
        &title,
        &blocks,
        &requested,
        title_override.as_deref(),
    )?;
    let bold_changed = postprocess::apply_markdown_bold(&root, &header_root)?;
    let parenthesis_changed = postprocess::apply_parenthesis_size(&root, &header_root)?;
    let superscript_changed = postprocess::apply_superscript(&root, &header_root)?;
    // 붙은 별표는 run 재구성 후처리들이 모두 지나간 뒤에 분할해야 한다.
    let asterisk_changed = postprocess::apply_attached_asterisk(&root, &header_root)?;
    // 15pt(휴먼명조) 문단의 줄넘침은 run 재구성이 끝난 뒤 판정한다.
    let linefit_changed = linefit::apply_line_fit(&root, &header_root)?;
    let header_xml = if bold_changed
        || parenthesis_changed
        || superscript_changed
        || asterisk_changed
        || linefit_changed
    {
        Some(xmltree::serialize(&header_root, true).into_bytes())
    } else {
        None
    };
    let section_xml = xmltree::serialize(&root, true).into_bytes();
    let compiled = hwpx::rewrite_hwpx_bytes(template_bytes, &section_xml, header_xml.as_deref())?;
    let validation = hwpx::validate_hwpx_bytes(&compiled)?;
    let mut variant_map = serde_json::Map::new();
    for (k, v) in variants {
        variant_map.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    if let Some(name) = source_variant {
        variant_map.insert("title".to_string(), serde_json::Value::String(name));
    }
    Ok((
        compiled,
        Report {
            output: String::new(),
            title: title.to_string(),
            block_count: blocks.len(),
            variants: variant_map,
            validation,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_xml_tags(xml: &str) -> String {
        let mut out = String::with_capacity(xml.len());
        let mut chars = xml.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '<' {
                for c2 in chars.by_ref() {
                    if c2 == '>' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn real_template() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/web/public/template.hwpx"
        ))
        .expect("web/public/template.hwpx")
    }

    #[test]
    fn depth_item_without_label_keeps_single_space_after_glyph() {
        let template = real_template();
        // 라벨 없는 depth 항목: 글머리(□/○)와 본문 사이 공백은 한 칸이어야 한다.
        let md = "제목: T\n\n□ 라벨 없는 항목\n\n○ 라벨 없는 둘째\n";
        let (bytes, _) = compile_hwpx_bytes(md, &template, &[]).expect("compile must succeed");
        let (root, _) = crate::compile::hwpx::read_template_parts_bytes(&bytes, "test").unwrap();
        let xml = crate::shared::xmltree::serialize(&root, true);
        // run 경계에 tag가 끼므로 태그를 제거한 평문으로 검사한다.
        let plain = strip_xml_tags(&xml);
        assert!(plain.contains("□ 라벨 없는 항목"), "{}", plain);
        assert!(plain.contains("○ 라벨 없는 둘째"), "{}", plain);
        assert!(!plain.contains("□  "), "double space after □: {}", plain);
        assert!(!plain.contains("○  "), "double space after ○: {}", plain);
    }

    #[test]
    fn mid_document_attach_title_keeps_report_title() {
        let template = real_template();
        let md = "보험계약 관련 검토 보고서\n\n본문 첫 문단입니다.\n\n[[붙임]] 별도계약 손실위험 안내\n\n붙임 본문입니다.\n";
        let (bytes, report) =
            compile_hwpx_bytes(md, &template, &[]).expect("compile must succeed");
        // 문서 제목은 그대로고, 표지 변형이 요청되지 않는다.
        assert_eq!(report.title, "보험계약 관련 검토 보고서");
        assert!(!report.variants.contains_key("title"), "{:?}", report.variants);

        // section0.xml에서 텍스트 순서 검증: 표지 제목 → 본문 → 붙임 표지 → 붙임 본문.
        let (root, _) = crate::compile::hwpx::read_template_parts_bytes(&bytes, "test").unwrap();
        let xml = crate::shared::xmltree::serialize(&root, true);
        let at = |needle: &str| xml.find(needle).expect(needle);
        let title_pos = at("보험계약 관련 검토 보고서");
        let body_pos = at("본문 첫 문단입니다.");
        let attach_pos = at("별도계약 손실위험 안내");
        let attach_body_pos = at("붙임 본문입니다.");
        assert!(
            title_pos < body_pos && body_pos < attach_pos && attach_pos < attach_body_pos,
            "attach title must be rendered in place: title={title_pos} body={body_pos} attach={attach_pos} attach_body={attach_body_pos}"
        );
        // 붙임 표지 문단은 '붙 임' 라벨을 함께 가진다.
        assert!(xml.contains("붙 임"), "attach cover label missing");
    }

    #[test]
    fn attach_title_at_document_head_still_swaps_cover() {
        let template = real_template();
        let md = "[[붙임]] 별도계약 손실위험 안내\n\n본문입니다.\n";
        let (bytes, report) =
            compile_hwpx_bytes(md, &template, &[]).expect("compile must succeed");
        assert_eq!(
            report.variants.get("title").and_then(|v| v.as_str()),
            Some("attachment")
        );
        let (root, _) = crate::compile::hwpx::read_template_parts_bytes(&bytes, "test").unwrap();
        let xml = crate::shared::xmltree::serialize(&root, true);
        // 표지가 붙임 변형으로 바뀌고 셀 텍스트가 명령 텍스트다.
        assert!(xml.contains("붙 임"));
        assert!(xml.contains("별도계약 손실위험 안내"));
    }
}
