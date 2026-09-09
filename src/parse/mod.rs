//! HWPX 바이트 → 마크다운 문자열 인메모리 변환 공개 API.
//! 파일 I/O는 없으며, CLI(main.rs)와 이후 Wasm(web/wasm)이 함께 쓴다.

pub use crate::shared::error;
pub mod cli;
pub mod hwpx;
pub mod parse;
pub mod postprocess;
pub mod report;
pub use crate::shared::xmltree;

// dialect와 glyph는 common 크레이트의 단일 출처를 re-export한다.
// hwpx2md가 쓰지 않는 항목(glyph의 split_marker 등)까지 통째로 공유하므로
// dead_code 경고를 끈다.
#[allow(dead_code, unused_imports)]
mod dialect;
#[allow(unused_imports)]
use crate::shared::glyph;

pub use error::{AppError, Result};

/// CLI/Wasm 공용 진입점. `label`은 오류 메시지에 쓰이는 입력 출처(파일
/// 경로 등)다.
pub fn hwpx_to_markdown_bytes(bytes: &[u8], label: &str) -> Result<(String, report::Report)> {
    let section_roots = hwpx::read_sections(bytes, label)?;
    if section_roots.is_empty() {
        return Err(AppError(format!("{label}: no HWPX section files found")));
    }
    let (blocks, stats) = parse::sections_to_blocks(&section_roots);
    let raw = blocks.join("\n\n");
    let markdown = postprocess::postprocess_markdown(&raw);
    Ok((markdown, report::build(section_roots.len(), blocks.len(), stats)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::hwpx::test_fixtures::fixture_hwpx;

    #[test]
    fn converts_plain_paragraph_document() {
        let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:t>제목</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>본문 텍스트</hp:t></hp:run></hp:p></hp:hs>"#;
        let bytes = fixture_hwpx(&[("Contents/section0.xml", section)]);
        let (markdown, report) = hwpx_to_markdown_bytes(&bytes, "test.hwpx").unwrap();
        assert!(markdown.contains("제목"));
        assert!(markdown.contains("본문 텍스트"));
        assert_eq!(report.sections, 1);
        assert_eq!(report.paragraph_count, 2);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn missing_sections_is_a_hard_error() {
        let bytes = fixture_hwpx(&[]);
        let e = hwpx_to_markdown_bytes(&bytes, "empty.hwpx").unwrap_err();
        assert!(e.0.contains("no HWPX section files found"), "{}", e.0);
    }

    #[test]
    fn memo_fields_do_not_leak_into_output() {
        let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:fieldBegin type="MEMO" id="m1"/><hp:t>숨긴 메모</hp:t><hp:fieldEnd beginIDRef="m1"/></hp:run><hp:run><hp:t>보이는 텍스트</hp:t></hp:run></hp:p></hp:hs>"#;
        let bytes = fixture_hwpx(&[("Contents/section0.xml", section)]);
        let (markdown, report) = hwpx_to_markdown_bytes(&bytes, "test.hwpx").unwrap();
        assert!(markdown.contains("보이는 텍스트"));
        assert!(!markdown.contains("숨긴 메모"));
        assert_eq!(report.paragraph_count, 1);
    }
}
