pub mod shared;
pub mod compile;
pub mod parse;

pub use shared::error::AppError;
pub use compile::{compile_hwpx_bytes, compile_hwpx_labeled};
pub use parse::hwpx_to_markdown_bytes;

#[cfg(test)]
mod tests {
    use super::*;

    /// 커밋된 실제 템플릿(README 수동 스모크와 동일 파일).
    static TEMPLATE: &[u8] = include_bytes!("../web/public/template.hwpx");

    fn compile(md: &str) -> Vec<u8> {
        compile_hwpx_bytes(md, TEMPLATE, &[]).unwrap().0
    }

    fn convert(bytes: &[u8]) -> String {
        hwpx_to_markdown_bytes(bytes, "roundtrip.hwpx").unwrap().0
    }

    #[test]
    fn markdown_numbered_heading_compiles_identically_to_bare_form() {
        // `## N. 제목`은 평문 `N. 제목`과 완전히 같은 HWPX를 낳는다.
        let bare = "# 제목\n\n1. 개요\n- 항목";
        let marked = "# 제목\n\n## 1. 개요\n- 항목";
        assert_eq!(compile(bare), compile(marked));
    }

    #[test]
    fn markdown_numbered_heading_round_trip_is_fixed() {
        // `## 1. 개요` → hwpx → md가 `## 1. 개요`로 고정되고, 다시 컴파일한
        // HWPX도 첫 컴파일과 바이트로 같다(같은 실행이면 표지 날짜도 같다).
        let hwpx1 = compile("# 제목\n\n## 1. 개요\n- 항목");
        let md = convert(&hwpx1);
        assert!(md.contains("## 1. 개요"), "md:\n{md}");
        let hwpx2 = compile(&md);
        assert_eq!(convert(&hwpx2), md);
        assert_eq!(hwpx2, hwpx1);
    }

    #[test]
    fn subheading_round_trip_keeps_stripped_form() {
        // 안정성 함정: 한 사이클 뒤 소제목 텍스트는 벗겨진 `1 세부`다.
        // 재역변환해도 `### ` 형태가 유지되어야 한다(표 채널이 셀을
        // 그대로 보존하므로 문단 줄 `## N.` 승격에 닿지 않는다).
        // `## 1 세부` 포함 검사는 `"### 1 세부"`가 부분문자열로 걸리므로
        // 줄 단위로 판정한다.
        let hwpx1 = compile("# 제목\n\n### 1 세부\n- 항목");
        let md = convert(&hwpx1);
        assert!(md.contains("### 1 세부"), "md:\n{md}");
        assert!(
            !md.lines().any(|l| l.trim() == "## 1 세부"),
            "md:\n{md}"
        );
        let hwpx2 = compile(&md);
        assert_eq!(convert(&hwpx2), md);
        assert_eq!(hwpx2, hwpx1);
    }

    #[test]
    fn bare_compound_subheading_converges_to_markdown_form() {
        // 구형 평문 입력 `2.1 세부`도 첫 사이클에 `### 1 세부` 정규 형태로
        // 수렴한다(표 채널도 문단 채널과 같은 벗기기 규칙을 쓴다).
        let hwpx = compile("# 제목\n\n2.1 세부\n- 항목");
        let md = convert(&hwpx);
        assert!(md.contains("### 1 세부"), "md:\n{md}");
        assert_eq!(convert(&compile(&md)), md);
    }

    #[test]
    fn example_md_round_trip_reaches_markdown_fixed_point() {
        // README 수동 스모크와 같은 왕복: example.md → hwpx → md → hwpx → md가
        // md 수준에서 고정되고 블록(제목/헤딩/불릿/표/박스)이 손실되지 않는다.
        let example = include_str!("../web/public/example.md");
        let md1 = convert(&compile(example));
        let hwpx2 = compile(&md1);
        let md2 = convert(&hwpx2);
        assert_eq!(md1, md2, "md1:\n{md1}\n--- md2:\n{md2}");
        assert!(md1.contains("## 1. 개 요"), "md1:\n{md1}");
        assert!(md1.contains("## 2. 주요내용"), "md1:\n{md1}");
        assert_eq!(convert(&compile(&md2)), md2);
    }
}
