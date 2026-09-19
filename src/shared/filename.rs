//! CLI 출력 파일명 줄기(stem) 정리 공통 출처: 금지 문자·C0 제어 문자에
//! 더해 화면·경로 표시를 흐리는 zero-width/bidi 유니코드까지 제거한다.
//! web/src/lib/sanitize.ts와 같은 문자 클래스를 다룬다.

/// 파일명에 쓸 수 없는 문자·제어 문자·zero-width/bidi 문자를 제거하고
/// 앞뒤 공백과 마침표를 잘라낸다.
pub fn sanitize_file_stem(title: &str) -> String {
    title
        .chars()
        .filter(|c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                && !c.is_control()
                && !is_invisible_unicode(*c)
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

/// zero-width 비가시 문자와 bidi 제어 문자. `char::is_control`(Cc만 잡음)은
/// 이 범위(Cf/Zs 계열)를 놓치므로 별도로 분류한다.
fn is_invisible_unicode(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200D}'    // zero-width space / ZWNJ / ZWJ
        | '\u{2060}'               // word joiner
        | '\u{FEFF}'               // zero-width no-break space (BOM)
        | '\u{180E}'               // mongolian vowel separator
        | '\u{202A}'..='\u{202E}'  // bidi embedding/override + pop 방향 제어
        | '\u{2066}'..='\u{2069}'  // bidi isolate 방향 제어
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_title_for_file_name() {
        assert_eq!(sanitize_file_stem(" A/B: C*D?E "), "AB CDE");
        assert_eq!(sanitize_file_stem("제목(안)."), "제목(안)");
        assert_eq!(sanitize_file_stem("제<목>"), "제목");
        assert_eq!(sanitize_file_stem(":*?"), "");
    }

    #[test]
    fn strips_zero_width_and_bidi_characters() {
        // U+202E(RLO) 등은 파일명 표시 방향을 뒤집어 확장자를 위장할 수
        // 있어 내용물과 무관하게 줄기에서는 모두 제거한다.
        let tricky = "\u{202E}보\u{200B}고\u{FEFF}서\u{2066}\u{202D}.hwpx\u{200D}";
        let stem = sanitize_file_stem(tricky);
        assert_eq!(stem, "보고서.hwpx");
        assert!(!stem.chars().any(is_invisible_unicode));
    }

    #[test]
    fn strips_word_joiner_and_mongolian_separator() {
        assert_eq!(sanitize_file_stem("가\u{2060}나\u{180E}다"), "가나다");
    }
}
