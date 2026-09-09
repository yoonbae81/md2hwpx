use std::sync::LazyLock;

use regex::Regex;

/// 소스 방언의 리터럴 토큰. 문법을 바꿀 때는 이 값들과
/// prompt.md·example.md·README의 문법 문서를 함께 고쳐야 하며,
/// `dialect_tokens_are_documented` 테스트가 빠뜨린 문서를 잡아준다.
pub mod dialect {
    pub use crate::shared::dialect::*;
}

/// hwp.py의 공용 정규식 모음.
pub static TEMPLATE_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\[\[(title|heading_default|heading|depth1|depth2|depth3|asterisk|reference|box|highlight|table)(?::([^:=\]\s]+))?\]\]",
    )
    .unwrap()
});

/// 템플릿의 날짜는 fwSpace가 일반 공백으로 흡수된 형태(`’26. 8. 25(금)`)로
/// 올 수 있으므로 자릿수 사이 공백을 허용한다.
pub static TEMPLATE_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[’']\d{2}\.\s*\d{1,2}\.\s*\d{1,2}\s*\([월화수목금토일]\)").unwrap()
});

pub static MARKDOWN_BOLD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());

pub static PARENTHESIS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\([^()\r\n]*\)|（[^（）\r\n]*）").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_regex_matches_spaced_and_compact_forms() {
        // fwSpace가 공백으로 흡수된 형태와 원래 조밀한 형태 모두 치환 대상이다.
        assert!(TEMPLATE_DATE_RE.is_match("’26. 8. 25(금)"));
        assert!(TEMPLATE_DATE_RE.is_match("’26.8.25(금)"));
        assert!(TEMPLATE_DATE_RE.is_match("'25.12.31(수)"));
        assert!(!TEMPLATE_DATE_RE.is_match("26.8.25(금)"));
    }

    #[test]
    fn dialect_tokens_are_documented() {
        // 문법 토큰을 바꿨다면 문서도 함께 고쳤는지 확인한다(드리프트 방지).
        // 모노레포 재배치 후 문서는 루트 README와 web 에셋에 있다.
        let readme = include_str!("../../README.md");
        let prompt = include_str!("../../web/public/prompt.md");
        let example = include_str!("../../web/public/example.md");
        assert_ne!(dialect::HIGHLIGHT, dialect::HRULE);
        assert!(
            readme.contains(dialect::HIGHLIGHT),
            "README에 highlight 토큰 문서가 없다"
        );
        assert!(
            prompt.contains(dialect::HIGHLIGHT),
            "prompt.md에 highlight 토큰 규칙이 없다"
        );
        assert!(
            example.lines().any(|l| l.trim() == dialect::HIGHLIGHT),
            "example.md에 highlight 예시가 없다"
        );
        assert!(
            readme.contains(dialect::HRULE),
            "README에 가로선 규칙 문서가 없다"
        );
        for token in [dialect::BOX, dialect::ATTACH, dialect::EXTERNAL] {
            assert!(
                readme.contains(token) && prompt.contains(token),
                "문서에 {token} 규칙이 없다"
            );
        }
        assert!(
            example.contains(dialect::BOX),
            "example.md에 박스 명령 예시가 없다"
        );
    }
}
