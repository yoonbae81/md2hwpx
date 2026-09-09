use std::collections::{BTreeSet, HashSet};
use std::io::{Cursor, Read, Write};

use crate::shared::zip_read::{check_package_size, contains_sub, reject_dtd, HWPX_MAX_PACKAGE_BYTES};

use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive};

use crate::shared::error::{AppError, Result};
use crate::shared::xmltree::{find_children, iter_elements, iter_tag, parse, text_nodes, HH, HP, NodeRef};
use crate::compile::patterns::TEMPLATE_MARKER_RE;
use crate::compile::template::SAMPLE_TEXT;

pub const HWPX_MIMETYPE: &str = "application/hwp+zip";
pub const HWPX_REQUIRED_PARTS: [&str; 4] = [
    "mimetype",
    "version.xml",
    "Contents/content.hpf",
    "Contents/header.xml",
];

fn section_name_re() -> &'static regex::Regex {
    use std::sync::LazyLock;
    static RE: std::sync::LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^Contents/section\d+\.xml$").unwrap());
    &RE
}

/// zip과 엔티티 안전 검사를 거쳐 템플릿의 section0/header를 읽는다.
/// `label`은 오류 메시지에 쓰이는 템플릿 출처(파일 경로 또는 "template")다.
pub fn read_template_parts_bytes(bytes: &[u8], label: &str) -> Result<(NodeRef, NodeRef)> {
    let mut package = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError(format!("{label}: {e}")))?;
    check_package_size(&mut package, label)?;
    let mut read_part = |name: &str| -> Result<Vec<u8>> {
        let mut f = package.by_name(name).map_err(|_| {
            AppError(format!("{label} is missing required part '{name}'"))
        })?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    };
    let section_bytes = read_part("Contents/section0.xml")?;
    let header_bytes = read_part("Contents/header.xml")?;
    drop(package);
    reject_dtd(&section_bytes, "Contents/section0.xml")?;
    reject_dtd(&header_bytes, "Contents/header.xml")?;
    let root = parse(&section_bytes)
        .map_err(|e| AppError(format!("malformed XML in {label}: {e}")))?;
    let header_root = parse(&header_bytes)
        .map_err(|e| AppError(format!("malformed XML in {label}: {e}")))?;
    Ok((root, header_root))
}

/// (항목 이름, 수정 시각, unix 모드, 내용)
type PackageEntry = (String, Option<zip::DateTime>, Option<u32>, Vec<u8>);

/// 템플릿 패키지를 복제하며 section0/header만 새 내용으로 교체한다.
/// 출력은 파일이 아니라 메모리 버퍼로 돌려준다(Wasm/CLI 공용).
pub fn rewrite_hwpx_bytes(
    template_bytes: &[u8],
    section_xml: &[u8],
    header_xml: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut zin = ZipArchive::new(Cursor::new(template_bytes))?;
    let mut entries: Vec<PackageEntry> = Vec::new();
    for i in 0..zin.len() {
        let mut zf = zin.by_index(i)?;
        let name = zf.name().to_string();
        let mtime = zf.last_modified();
        let unix_mode = zf.unix_mode();
        let data = if name == "Contents/section0.xml" {
            section_xml.to_vec()
        } else if name == "Contents/header.xml" {
            match header_xml {
                Some(h) => h.to_vec(),
                None => {
                    let mut buf = Vec::new();
                    zf.read_to_end(&mut buf)?;
                    buf
                }
            }
        } else {
            let mut buf = Vec::new();
            zf.read_to_end(&mut buf)?;
            buf
        };
        entries.push((name, mtime, unix_mode, data));
    }

    // large_file(true)를 쓰면 모든 항목에 Zip64 extra field가 붙어
    // 한글 프로그램이 열지 못한다. 템플릿과 같은 일반 zip(버전 20)으로
    // 유지한다.
    let mut output_bytes = Vec::new();
    {
        let mut zw = zip::ZipWriter::new(Cursor::new(&mut output_bytes));
        for (name, mtime, unix_mode, data) in &entries {
            let mut options = SimpleFileOptions::default()
                .compression_method(if name == "mimetype" {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                });
            if let Some(dt) = mtime {
                options = options.last_modified_time(*dt);
            }
            if let Some(mode) = unix_mode {
                options = options.unix_permissions(*mode);
            }
            zw.start_file(name, options)?;
            zw.write_all(data)?;
        }
        zw.finish()?;
    }
    Ok(output_bytes)
}

#[derive(Serialize)]
pub struct Validation {
    pub format: &'static str,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
    #[serde(rename = "paraCount")]
    pub para_count: usize,
    pub sections: usize,
    pub title: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// zip의 mimetype 항목이 첫 항목이고 STORED에 올바른 내용인지 검사한다.
fn check_mimetype<R: Read + std::io::Seek>(
    package: &mut ZipArchive<R>,
    infos: &[(String, CompressionMethod, u64)],
    errors: &mut Vec<String>,
) -> Result<()> {
    let Some((name, compression, _)) = infos.first() else {
        errors.push("mimetype entry is not first.".into());
        return Ok(());
    };
    if name != "mimetype" {
        errors.push("mimetype entry is not first.".into());
        return Ok(());
    }
    if *compression != CompressionMethod::Stored {
        errors.push("mimetype entry is not STORED.".into());
        return Ok(());
    }
    let mut f = package.by_name("mimetype")?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    if String::from_utf8_lossy(&buf) != HWPX_MIMETYPE {
        errors.push(format!("mimetype is not {HWPX_MIMETYPE}."));
    }
    Ok(())
}

/// 패키지의 XML 파트를 읽어 DTD를 거르고 section/header 트리를 모은다.
fn collect_xml_parts<R: Read + std::io::Seek>(
    package: &mut ZipArchive<R>,
    infos: &[(String, CompressionMethod, u64)],
    errors: &mut Vec<String>,
    header_root: &mut Option<NodeRef>,
    section_roots: &mut Vec<NodeRef>,
) -> Result<()> {
    for (name, _, _) in infos {
        if !name.ends_with(".xml") {
            continue;
        }
        let mut f = package.by_name(name)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        if contains_sub(&data, b"<!DOCTYPE") || contains_sub(&data, b"<!ENTITY") {
            errors.push(format!("{name} contains a DTD/entity declaration."));
            continue;
        }
        match parse(&data) {
            Err(e) => errors.push(format!("failed to parse XML in {name}: {e}")),
            Ok(parsed) => {
                if name == "Contents/header.xml" {
                    *header_root = Some(parsed);
                } else if section_name_re().is_match(name) {
                    section_roots.push(parsed);
                }
            }
        }
    }
    Ok(())
}

/// section이 참조한 charPrIDRef가 header에 모두 있는지 검사한다.
fn cross_check_char_pr_refs(
    header_root: &NodeRef,
    section_roots: &[NodeRef],
    errors: &mut Vec<String>,
) {
    let known_ids: HashSet<String> = iter_tag(header_root, HH, "charPr")
        .iter()
        .filter_map(|c| c.borrow().get_attr("id"))
        .collect();
    for section_root in section_roots {
        let mut missing: BTreeSet<String> = BTreeSet::new();
        for node in iter_elements(section_root) {
            if let Some(r) = node.borrow().get_attr("charPrIDRef") {
                if !known_ids.contains(&r) {
                    missing.insert(r);
                }
            }
        }
        if !missing.is_empty() {
            errors.push(format!(
                "charPrIDRef missing from header.xml: {}",
                missing.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }
}

/// 섹션 문단 수 집계와 남은 메모/표식/샘플 텍스트를 검사한다.
fn scan_section_contents(
    section_roots: &[NodeRef],
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> usize {
    let mut para_count = 0usize;
    for section_root in section_roots {
        para_count += find_children(section_root, HP, "p").len();
        let has_memo = iter_tag(section_root, HP, "fieldBegin")
            .iter()
            .any(|n| n.borrow().get_attr("type").as_deref() == Some("MEMO"));
        if has_memo {
            errors.push("MEMO field control remains in the output.".into());
        }
        for node in text_nodes(section_root) {
            let text = node.borrow().text_or_empty();
            if let Some(m) = TEMPLATE_MARKER_RE.find(&text) {
                errors.push(format!("template marker remains: {}", m.as_str()));
            }
            if text.contains(SAMPLE_TEXT) {
                warnings.push("prototype sample text remains.".into());
            }
        }
    }
    para_count
}

/// 첫 섹션에서 첫 비어 있지 않은 문단 텍스트를 제목으로 읽는다.
fn extract_title(section_roots: &[NodeRef]) -> String {
    let Some(first) = section_roots.first() else {
        return String::new();
    };
    for paragraph in iter_tag(first, HP, "p") {
        let text: String = iter_tag(&paragraph, HP, "t")
            .iter()
            .map(|n| n.borrow().text_or_empty())
            .collect();
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    String::new()
}

/// 외부 도구 없이 패키지 구조와 생성 XML을 검증한다.
pub fn validate_hwpx_bytes(bytes: &[u8]) -> Result<Validation> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut section_roots: Vec<NodeRef> = Vec::new();

    let mut package = ZipArchive::new(Cursor::new(bytes))?;
    let mut names: HashSet<String> = HashSet::new();
    let mut infos: Vec<(String, CompressionMethod, u64)> = Vec::new();
    for i in 0..package.len() {
        let f = package.by_index(i)?;
        names.insert(f.name().to_string());
        infos.push((f.name().to_string(), f.compression(), f.size()));
    }
    check_mimetype(&mut package, &infos, &mut errors)?;
    for part in HWPX_REQUIRED_PARTS {
        if !names.contains(part) {
            errors.push(format!("missing required part: {part}"));
        }
    }
    let total: u64 = infos.iter().map(|(_, _, s)| *s).sum();
    let mut para_count = 0usize;
    if total > HWPX_MAX_PACKAGE_BYTES {
        errors.push("package uncompressed size is too large.".into());
    } else {
        let mut header_root: Option<NodeRef> = None;
        collect_xml_parts(&mut package, &infos, &mut errors, &mut header_root, &mut section_roots)?;
        if let Some(hr) = &header_root {
            cross_check_char_pr_refs(hr, &section_roots, &mut errors);
        }
        para_count = scan_section_contents(&section_roots, &mut errors, &mut warnings);
    }
    let title = extract_title(&section_roots);
    Ok(Validation {
        format: "hwpx",
        size_bytes: bytes.len() as u64,
        para_count,
        sections: section_roots.len(),
        title,
        warnings,
        errors,
    })
}

#[derive(Serialize)]
pub struct Report {
    pub output: String,
    pub title: String,
    #[serde(rename = "blockCount")]
    pub block_count: usize,
    pub variants: serde_json::Map<String, serde_json::Value>,
    pub validation: Validation,
}
