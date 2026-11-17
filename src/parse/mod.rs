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
    let superscripts = hwpx::read_superscript_charprs(bytes, label)?;
    let (blocks, stats) = parse::sections_to_blocks(&section_roots, &superscripts);
    let raw = blocks.join("\n\n");
    let markdown = postprocess::postprocess_markdown(&raw);
    Ok((markdown, report::build(section_roots.len(), blocks.len(), stats)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::hwpx::test_fixtures::{fixture_hwpx, fixture_hwpx_with_header};

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

    #[test]
    fn superscript_closing_caret_restored_only_when_text_follows() {
        // 닫는 윗첨자 표식은 공백 없이 다른 글자가 바로 붙을 때만 복원한다.
        // 공백·문단 끝 뒤에는 `^` 하나면 왕복 손실이 없고, 붉은 별표 변형은
        // 윗첨자 표식 대상이 아니다.
        let header = r##"<?xml version="1.0" encoding="utf-8"?><hh:header xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head"><hh:charProperties itemCnt="2"><hh:charPr id="7"><hh:supscript/></hh:charPr><hh:charPr id="8" textColor="#FF0000"><hh:supscript/></hh:charPr></hh:charProperties></hh:header>"##;
        let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:t>앞</hp:t></hp:run><hp:run charPrIDRef="7"><hp:t>둘</hp:t></hp:run><hp:run><hp:t>만</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>앞</hp:t></hp:run><hp:run charPrIDRef="7"><hp:t>둘</hp:t></hp:run><hp:run><hp:t> 뒤</hp:t></hp:run></hp:p><hp:p><hp:run charPrIDRef="7"><hp:t>끝</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>빨강</hp:t></hp:run><hp:run charPrIDRef="8"><hp:t>*</hp:t></hp:run><hp:run><hp:t>다음</hp:t></hp:run></hp:p><hp:p><hp:run><hp:t>결과는 </hp:t></hp:run><hp:run charPrIDRef="7"><hp:t>②</hp:t></hp:run><hp:run><hp:t>. 그래서</hp:t></hp:run></hp:p></hp:hs>"#;
        let bytes = fixture_hwpx_with_header(&[("Contents/section0.xml", section)], header);
        let (markdown, _) = hwpx_to_markdown_bytes(&bytes, "test.hwpx").unwrap();
        assert!(markdown.contains("앞^둘^만"), "{}", markdown);
        assert!(markdown.contains("앞^둘 뒤"), "{}", markdown);
        assert!(markdown.contains("^끝"), "{}", markdown);
        assert!(!markdown.contains("^끝^"), "{}", markdown);
        assert!(markdown.contains("빨강*다음"), "{}", markdown);
        assert!(!markdown.contains("빨강^*다음"), "{}", markdown);
        // 마침표는 종결 문자라 닫는 표식 없이도 왕복이 무손실이다.
        assert!(markdown.contains("결과는 ^②. 그래서"), "{}", markdown);
        assert!(!markdown.contains("^②^. 그래서"), "{}", markdown);
    }

    #[test]
    fn page_number_control_is_not_imported() {
        // 본문에 페이지 번호 컨트롤(hp:pageNum)만 남은 문서는 빈 출력이다.
        let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run charPrIDRef="10"><hp:ctrl><hp:pageNum pos="BOTTOM_CENTER" formatType="DIGIT" sideChar="-"/></hp:ctrl><hp:t/></hp:run></hp:p></hp:hs>"#;
        let bytes = fixture_hwpx(&[("Contents/section0.xml", section)]);
        let (markdown, report) = hwpx_to_markdown_bytes(&bytes, "page-no.hwpx").unwrap();
        assert!(markdown.trim().is_empty(), "{}", markdown);
        assert_eq!(report.paragraph_count, 0);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn body_text_around_page_number_control_survives() {
        // 페이지 번호 컨트롤은 무시하되 같은 문단의 실제 본문은 살린다.
        let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:ctrl><hp:pageNum pos="BOTTOM_CENTER" formatType="DIGIT" sideChar="-"/></hp:ctrl><hp:t/></hp:run><hp:run><hp:t>본문 텍스트</hp:t></hp:run></hp:p></hp:hs>"#;
        let bytes = fixture_hwpx(&[("Contents/section0.xml", section)]);
        let (markdown, report) = hwpx_to_markdown_bytes(&bytes, "test.hwpx").unwrap();
        assert!(markdown.contains("본문 텍스트"));
        assert_eq!(report.paragraph_count, 1);
    }
}
