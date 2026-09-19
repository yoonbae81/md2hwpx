//! 항목 앞에 놓이는 글리프(불릿 기호) 규칙의 단일 출처.
//!
//! `template.hwpx`에는 담을 수 없고 소스 텍스트 해석과 출력 불릿 결정에
//! 걸쳐 있는 규칙이다. 소스 파서(`source.rs`)와 렌더러(`render.rs`)가
//! 글리프 규칙을 볼 때는 이 모듈만 본다. 새 글리프는 `BULLET_GLYPHS`와
//! 필요 시 `is_note_glyph`에 한 줄 추가하면 파서와 렌더러가 함께 따라간다.

/// 불릿으로 인정하는 글리프. 소스 문서마다 쓰는 기호가 다르므로 한정하지
/// 않고, 깊이는 상대 들여쓰기로 판정한다. 주석 글리프도 불릿 자리를
/// 차지해 들여쓰기 깊이 판정에 참여한다.
const BULLET_GLYPHS: &[char] = &[
    '-', '*', '+', '○', 'ㅇ', '◯', '●', '◦', '•', '·', '∙', '∘', '▪', '▫', '‣', '⁃', '□', '■',
    '◆', '◇', '▶', '▷', '⊙', '☞',
];

/// 불릿 없이 들여쓰기만 하는 주석 글리프. 항목 본문 앞에 남아 그대로
/// 출력되고 출력 불릿은 비운다.
pub fn is_note_glyph(c: char) -> bool {
    matches!(c, '☞')
}

/// 원문자 번호(①~⑳). 불릿 글리프로도 쓰고, 항목 본문 앞에 남으면 그 번호가
/// 곧 출력 불릿 문자가 된다(template 불릿으로 정규화하지 않는다).
pub fn is_circled_digit(c: char) -> bool {
    ('①'..='⑳').contains(&c)
}

/// 글리프가 항목에서 맡는 역할.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphRole {
    /// 템플릿 불릿으로 정규화되는 일반 불릿 기호. 본문에 남지 않는다.
    Bullet,
    /// 원문자 번호. 본문 앞에 남고 그 번호가 곧 출력 불릿이 된다.
    CircledNumber,
    /// 주석 글리프. 본문 앞에 남고 출력 불릿은 비운다.
    Note,
}

impl GlyphRole {
    /// 항목 본문 앞에 남아야 하는 글리프인지. 남는 글리프는 파서가 본문을
    /// 재구성하고 렌더러(`split_marker`)가 출력 불릿을 다시 정한다.
    pub fn stays_in_body(self) -> bool {
        matches!(self, GlyphRole::CircledNumber | GlyphRole::Note)
    }
}

/// 글리프를 역할로 분류한다. None은 불릿 항목이 아니다.
pub fn classify(c: char) -> Option<GlyphRole> {
    if is_circled_digit(c) {
        return Some(GlyphRole::CircledNumber);
    }
    if is_note_glyph(c) {
        return Some(GlyphRole::Note);
    }
    if BULLET_GLYPHS.contains(&c) {
        return Some(GlyphRole::Bullet);
    }
    None
}

/// 불릿 자리를 차지하는 글리프인지(일반 불릿 + 원문자 번호 + 주석 글리프).
pub fn is_bullet_glyph(c: char) -> bool {
    classify(c).is_some()
}

/// 소스 항목 줄에서 글리프 뒤 본문을 만든다. '*'는 왼쪽 공백만 지우고
///(들여쓴 '*'의 보조표기 용도 보존) 나머지 글리프는 양쪽을 지운다.
/// 본문에 남는 글리프(원문자 번호·주석)는 본문 앞에 되살린다. 깊이 항목과
/// 박스 항목이 같은 규칙을 쓰도록 이곳에서만 판정한다.
pub fn bullet_item_body(glyph: char, rest: &str) -> String {
    let body = if glyph == '*' {
        rest.trim_start()
    } else {
        rest.trim()
    };
    if classify(glyph).is_some_and(|role| role.stays_in_body()) {
        return if body.is_empty() {
            glyph.to_string()
        } else {
            format!("{glyph} {body}")
        };
    }
    body.to_string()
}

/// 같은 들여쓰기의 다른 기호 바로 아래에서 오면 한 단계 더 깊게 보는
/// "보이는 하위 항목" 글리프: 하위 불릿(○/ㅇ)과 본문에 남는 글리프.
pub fn is_visible_sub_item(c: char) -> bool {
    matches!(c, '○' | 'ㅇ') || classify(c).is_some_and(|role| role.stays_in_body())
}

/// 항목의 글리프를 확정한다. 박스 등 다른 기호 뒤에 본문에 남는 글리프
/// (원문자 번호·주석)가 붙어 오면 그 글리프가 항목의 성격(깊이 규칙과
/// 출력 불릿)을 결정하고, 그 밖에는 원래 글리프를 유지한다.
pub fn resolve_item_glyph(glyph: char, body: &str) -> char {
    body.chars()
        .next()
        .filter(|&c| classify(c).is_some_and(|role| role.stays_in_body()))
        .unwrap_or(glyph)
}

/// 항목 본문의 첫 글리프가 출력 불릿을 결정한다. 원문자 번호는 그 번호가
/// 불릿이 되고, 주석 글리프(☞ 등)는 불릿을 비워 본문에 그대로 남긴다.
/// 그 외에는 템플릿 불릿으로 정규화한다.
pub fn split_marker(value: &str, template_marker: &str) -> (String, String) {
    match value.chars().next() {
        Some(c) if is_circled_digit(c) => {
            let rest = value[c.len_utf8()..].trim_start();
            (c.to_string(), rest.to_string())
        }
        Some(c) if is_note_glyph(c) => (String::new(), value.to_string()),
        _ => (template_marker.to_string(), value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_marker_uses_body_glyph_first() {
        assert_eq!(
            split_marker("① (인력수요) 내용", "□"),
            ("①".to_string(), "(인력수요) 내용".to_string())
        );
        assert_eq!(
            split_marker("①(인력수요)", "□"),
            ("①".to_string(), "(인력수요)".to_string())
        );
        assert_eq!(split_marker("①", "□"), ("①".to_string(), String::new()));
        // 주석 글리프는 출력 불릿을 비우고 본문에 그대로 남는다.
        assert_eq!(
            split_marker("☞ (사업영향) 내용", "□"),
            (String::new(), "☞ (사업영향) 내용".to_string())
        );
        assert_eq!(
            split_marker("(인력수요) 내용", "□"),
            ("□".to_string(), "(인력수요) 내용".to_string())
        );
        assert_eq!(
            split_marker("+550MW 증가", "□"),
            ("□".to_string(), "+550MW 증가".to_string())
        );
    }

    #[test]
    fn bullet_item_body_builds_body_per_glyph_role() {
        // 일반 불릿은 본문만 남고, 원문자 번호·주석 글리프는 본문 앞에 된다.
        assert_eq!(bullet_item_body('-', " 항목", ), "항목");
        assert_eq!(bullet_item_body('*', " *붙은 표기", ), "*붙은 표기");
        assert_eq!(bullet_item_body('①', " 번호 항목"), "① 번호 항목");
        assert_eq!(bullet_item_body('☞', " 주석"), "☞ 주석");
        // 본문이 비면 남는 글리프만 남는다.
        assert_eq!(bullet_item_body('①', "  "), "①");
        assert_eq!(bullet_item_body('-', "  "), "");
    }

    #[test]
    fn roles_decide_body_and_sub_item() {
        assert_eq!(classify('-'), Some(GlyphRole::Bullet));
        assert_eq!(classify('+'), Some(GlyphRole::Bullet));
        assert_eq!(classify('□'), Some(GlyphRole::Bullet));
        assert_eq!(classify('①'), Some(GlyphRole::CircledNumber));
        assert_eq!(classify('☞'), Some(GlyphRole::Note));
        assert_eq!(classify('가'), None);
        assert!(!GlyphRole::Bullet.stays_in_body());
        assert!(is_visible_sub_item('○'));
        assert!(is_visible_sub_item('ㅇ'));
        assert!(is_visible_sub_item('①'));
        assert!(is_visible_sub_item('☞'));
        assert!(!is_visible_sub_item('□'));
        assert!(!is_visible_sub_item('-'));
    }

    #[test]
    fn body_glyph_overrides_item_glyph_only_when_it_stays() {
        // 본문에 남는 글리프가 앞에 오면 그 글리프가 항목의 글리프다.
        assert_eq!(resolve_item_glyph('-', "① 내용"), '①');
        assert_eq!(resolve_item_glyph('□', "☞ 주석"), '☞');
        // 남는 글리프가 아니면 원래 글리프를 유지한다.
        assert_eq!(resolve_item_glyph('-', "□ 보통 불릿"), '-');
        assert_eq!(resolve_item_glyph('-', "(라벨) 내용"), '-');
        assert_eq!(resolve_item_glyph('□', ""), '□');
    }
}
