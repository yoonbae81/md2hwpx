//! 휴먼명조(HMKMM.TTF, 15pt 기준)의 문자 폭 테이블.
//!
//! HWP unit(1/7200in) @15pt. 함초롬바탕·휴먼명조 실측: 한글은 1em(1500),
//! ASCII는 비례 폭. kerning off(useKerning="0")이므로 advance의 단순 합이
//! 곧 렌더 폭이다. 출처: HMKMM.TTF의 cmap fmt4 + hmtx를 파싱해 추출했다.

/// 한글/한자/전각 기호 1글자 = 1em = 1500 unit.
pub const HANGUL_WIDTH: i64 = 1500;

/// 문자별 폭(HWP unit). 테이블에 없는 문자는 ASCII 기본 750(0.5em).
pub fn char_width(c: char) -> i64 {
    match c {
        ' ' | '0'..='9' | '-' | '"' | '+' | '*' | '=' | '<' | '>' | '_' | '~' => 750,
        '!' | '|' => 305,
        'i' | 'l' => 375,
        'I' => 387,
        'j' => 398,
        '.' | ',' | ':' | ';' => 404,
        '\'' => 434,
        '{' | '}' => 439,
        '(' | ')' | '/' => 469,
        '[' | ']' => 480,
        't' => 510,
        '^' => 545,
        'f' => 586,
        'J' => 592,
        'r' => 604,
        's' => 615,
        '?' => 645,
        'z' => 680,
        'c' => 727,
        'x' => 732,
        'S' => 744,
        '$' => 756,
        'a' | 'd' => 762,
        'o' | 'y' => 768,
        'g' => 773,
        'e' | 'k' => 785,
        'p' | 'q' | 'u' | 'v' => 809,
        'b' => 814,
        'n' => 826,
        'h' => 832,
        'P' => 861,
        'L' => 867,
        'F' => 873,
        'Z' => 896,
        'B' => 902,
        'E' | 'T' => 938,
        'C' => 961,
        'K' => 967,
        'R' => 979,
        '#' => 984,
        'A' => 990,
        'V' => 996,
        'Y' => 1008,
        'U' => 1014,
        'D' | 'H' => 1020,
        'Q' => 1031,
        'N' => 1037,
        'G' => 1055,
        'X' => 1066,
        'O' => 1072,
        'w' => 1084,
        '%' => 1125,
        '&' => 1154,
        '@' => 1166,
        'M' => 1172,
        'm' => 1195,
        'W' => 1400,
        _ => {
            if is_wide(c) {
                HANGUL_WIDTH
            } else {
                750
            }
        }
    }
}

/// 한글/한자/전각 기호(넓은 문자) 판정.
pub fn is_wide(c: char) -> bool {
    let u = c as u32;
    (0xAC00..=0xD7A3).contains(&u)          // Hangul syllables
        || (0x1100..=0x11FF).contains(&u)    // Hangul Jamo
        || (0x3130..=0x318F).contains(&u)    // Hangul Compatibility Jamo
        || (0x4E00..=0x9FFF).contains(&u)    // CJK Unified
        || (0x3400..=0x4DBF).contains(&u)    // CJK Extension A
        || (0xF900..=0xFAFF).contains(&u)    // CJK Compatibility
        || (0x25A0..=0x25FF).contains(&u)    // Geometric shapes □●○△
        || (0x2018..=0x201F).contains(&u)    // Curly quotes
        || (0x2010..=0x2027).contains(&u)    // General punctuation –—‥…
        || (0x3000..=0x303F).contains(&u)    // CJK Symbols ※
        || (0xFF00..=0xFF60).contains(&u)    // Fullwidth forms
        || (0x2460..=0x24FF).contains(&u)    // Enclosed alphanumerics ①
        || (0x2160..=0x217F).contains(&u)    // Roman numerals
}

/// 문자열의 폭(HWP unit) 합.
pub fn text_width(text: &str) -> i64 {
    text.chars().map(char_width).sum()
}
