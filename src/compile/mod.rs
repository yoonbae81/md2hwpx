//! 마크다운 + template.hwpx 바이트 → HWPX 바이트 인메모리 변환 공개 API.
//! 파일 I/O는 없으며, CLI(main.rs)와 Wasm(web/wasm)이 함께 쓴다.

pub mod cli;
pub mod hwpx;
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
    // 붙은 별표는 run 재구성 후처리들이 모두 지나간 뒤에 분할해야 한다.
    let asterisk_changed = postprocess::apply_attached_asterisk(&root, &header_root)?;
    let header_xml = if bold_changed || parenthesis_changed || asterisk_changed {
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
