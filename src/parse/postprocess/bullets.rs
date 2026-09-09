//! 불릿 계층 복원 (두 패스) — Python 참조의 문맥 패스+고정 패스 구조를
//! 유지하되, 내보내는 토큰은 md2hwpx 소스 방언에 맞춘다.
//!
//! 왕복 편차(Python 참조 대비): Python은 고정 글리프를 `+`/`*`/2칸/4칸
//! `-`의 글리프별 마크다운으로 매핑했지만, md2hwpx 소스 파서(source.rs의
//! `finish_bullets`)는 글리프 선택이 아니라 들여쓰기의 상대 순위로 깊이를
//! 정한다. 따라서 두 패스 모두 `- ` 불릿을 (깊이-1)×2칸 들여쓰기로 내보내고,
//! 글리프는 glyph.rs 분류상 본문에 남아야 하는 것(원문자 번호, 주석 `☞`)
//! 만 보존한다. Python의 3단계 `·` 글리프 치환 우회는 이 방식에서 불필요하다.
//!
//! 패스 1(문맥, `restore_bullet_levels`): 글리프와 원문 들여쓰기로 계층을
//! 추적해 문맥 글리프(`-`, 원문자 열거, 주석 글리프)를 정규화된 `- ` 줄로
//! 재작성한다. 고정 글리프(□/○/·)는 규약 깊이로 맥락에만 참여하고 패스 2가
//! 처리한다. 불릿이 아닌 줄이 끼면 맥락은 끊기고, 빈 줄은 맥락을 유지한다.
//! 패스 2(고정, `restore_bullet_depths`): 남은 고정 글리프를 같은
//! `- `+들여쓰기 형식으로 매핑한다. `*`/`•`/`※`는 각주·참고 표기라 여기서
//! 재작성하지 않는다(각주 별표 패스와 노트 표기 보존).

use crate::shared::glyph::{is_circled_digit, is_note_glyph, is_visible_sub_item, resolve_item_glyph};

/// 고정 글리프 불릿의 규약 깊이(Python 참조 `_FIXED_BULLET_DEPTH`와 같다).
/// `*`/`※`는 각주·참고 표식이라 재작성하지 않고, 계층 맥락 추적용으로만
/// 2단계로 기록한다.
fn fixed_bullet_depth(c: char) -> Option<usize> {
    match c {
        '□' | '■' | 'ㅁ' => Some(1),
        '○' | 'ㅇ' | '●' => Some(2),
        '·' | '‧' => Some(3),
        '*' | '※' => Some(2),
        _ => None,
    }
}

/// 패스 2가 재작성하는 고정 글리프. `*`/`•`는 각주 별표 패스가 다루고
/// `※`는 불릿이 아니므로 제외한다. ASCII `-`는 패스 1의 산출물이므로
/// 여기서 다시 만지면 3단계 들여쓰기가 평탄화된다 — 절대 포함하지 않는다.
fn rewritten_fixed_depth(c: char) -> Option<usize> {
    match c {
        '□' | '■' | 'ㅁ' => Some(1),
        '○' | 'ㅇ' | '●' => Some(2),
        '·' | '‧' => Some(3),
        _ => None,
    }
}

/// 원문자 열거(①②③, ㉠㉡…) — 열거 기호 자체가 의미를 갖므로 본문에
/// 보존한다. glyph.rs의 `is_circled_digit`(①~⑳)에 한글 원문자(㉠~㉿)
/// 범위를 더한 것(한글 원문자는 glyph.rs 분류 밖의 후처리 전용 관습).
fn is_circled_numeral(c: char) -> bool {
    is_circled_digit(c) || ('\u{3260}'..='\u{327F}').contains(&c)
}

/// 불릿 후보 줄 하나를 파싱한 결과.
struct BulletLine {
    glyph: char,
    /// 원문 들여쓰기(공백 문자 수).
    indent: usize,
    /// 글리프 뒤 본문 (선행 공백 제거).
    body: String,
    /// 고정 글리프의 규약 깊이 (문맥 글리프는 None).
    fixed_depth: Option<usize>,
    /// `-`와 원문자 열거, 주석 글리프 — 앞 불릿과의 상대 들여쓰기로 깊이 결정.
    contextual: bool,
}

/// 불릿 후보 줄을 파싱한다. 불릿 글리프(glyph.rs 분류 기준)나 고정 깊이
/// 관습에 해당하지 않으면 None.
fn parse_bullet_line(line: &str) -> Option<BulletLine> {
    let stripped = line.trim_start();
    if stripped.is_empty() {
        return None;
    }
    let glyph = stripped.chars().next()?;
    let fixed_depth = fixed_bullet_depth(glyph);
    let contextual = glyph == '-' || is_circled_numeral(glyph) || is_note_glyph(glyph);
    if fixed_depth.is_none() && !contextual {
        return None;
    }
    let indent = line.chars().count() - stripped.chars().count();
    let body = stripped[glyph.len_utf8()..].trim_start().to_string();
    Some(BulletLine {
        glyph,
        indent,
        body,
        fixed_depth,
        contextual,
    })
}

/// 문맥 글리프(`-`, ①②③…)의 계층 — 앞 불릿과의 상대 들여쓰기로 정한다.
///
/// 앞 불릿보다 깊게 들여써지면 한 단계 아래(최대 3단계), 같은 들여쓰기면
/// 형제, 더 얕으면 2단계다. 1단계는 □류 장식 불릿 전용이므로 최하 2단계다.
fn contextual_depth(
    indent: usize,
    prev_indent: Option<usize>,
    prev_depth: Option<usize>,
) -> usize {
    match (prev_indent, prev_depth) {
        (Some(pi), Some(pd)) => {
            if indent > pi {
                (pd + 1).min(3)
            } else if indent == pi {
                pd.max(2)
            } else {
                2
            }
        }
        _ => 2,
    }
}

/// 문맥 불릿 줄을 규약 깊이의 `- ` 줄로 재작성한다.
///
/// 본문에 남는 글리프(원문자 번호, 주석 `☞`)는 `- ① 본문` 형태로 본문 앞에
/// 보존한다 — md2hwpx의 source.rs가 항목 글리프를 이렇게 다시 확정한다.
fn render_bullet(bullet: &BulletLine, depth: usize) -> String {
    let indent_ws = " ".repeat((depth - 1) * 2);
    let keep_glyph = bullet.glyph != '-' && (is_circled_numeral(bullet.glyph) || is_note_glyph(bullet.glyph));
    if keep_glyph {
        format!("{}- {} {}", indent_ws, bullet.glyph, bullet.body)
    } else {
        format!("{}- {}", indent_ws, bullet.body)
    }
}

/// 불릿 글리프와 원문 들여쓰기로 마크다운 목록 계층을 복원한다 (문맥 패스).
///
/// 줄 파싱(`parse_bullet_line`) → 맥락 추적(앞 불릿과의 상대 들여쓰기) →
/// 재작성(`render_bullet`)의 단일 패스다. 고정 글리프(`□`/`○`/`·`류)는
/// 여기서 재작성하지 않고 규약 깊이로 맥락에만 참여한다 — 다음 고정 패스의
/// 대상이다.
pub(crate) fn restore_bullet_levels(markdown: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    // 직전 불릿의 (들여쓰기, 깊이, 글리프) — md2hwpx finish_bullets의 맥락.
    let mut prev: Option<(usize, usize, char)> = None;
    for line in markdown.split('\n') {
        match parse_bullet_line(line) {
            None => {
                if !line.trim().is_empty() {
                    prev = None;
                }
                out.push(line.to_string());
            }
            Some(bullet) => {
                // 들여쓴 `* `는 md2hwpx에서 각주(asterisk) 블록이지 불릿 항목이
                // 아니므로, 맥락을 갱신하지도 끊지도 않는다(투명하게 건너뛴다).
                // 이 줄이 불릿 맥락에 끼면 뒤 항목의 깊이가 잘못 눌리고, 그것이
                // 재흡수 시 "보이는 하위 항목" 범프 발동으로 이어진다.
                if bullet.glyph == '*' && bullet.indent > 0 {
                    out.push(line.to_string());
                    continue;
                }
                // 항목의 성격은 본문 첫 글리프가 결정할 수 있다(`- ☞ 본문`의
                // 항목 글리프는 ☞) — md2hwpx handle_body_line도 확정값을 기록한다.
                let effective = resolve_item_glyph(bullet.glyph, &bullet.body);
                let mut depth = bullet.fixed_depth.unwrap_or_else(|| {
                    if bullet.body.is_empty() {
                        2
                    } else {
                        contextual_depth(bullet.indent, prev.map(|(i, _, _)| i), prev.map(|(_, d, _)| d))
                    }
                });
                // "보이는 하위 항목" 범프(md2hwpx finish_bullets와 같은 규칙):
                // 같은 들여쓰기의 다른 기호 바로 아래에서 온 주석(☞)·원문자·하위
                // 불릿(○/ㅇ) 글리프는 한 단계 더 깊게 본다. 연달아 오면 같은
                // 단계를 유지한다.
                if is_visible_sub_item(effective) {
                    if let Some((pi, pd, pg)) = prev {
                        if pi == bullet.indent {
                            depth = if is_visible_sub_item(pg) {
                                pd
                            } else {
                                (depth + 1).min(3)
                            };
                        }
                    }
                }
                if bullet.contextual && !bullet.body.is_empty() {
                    out.push(render_bullet(&bullet, depth));
                } else {
                    out.push(line.to_string());
                }
                prev = Some((bullet.indent, depth, effective));
            }
        }
    }
    out.join("\n")
}

/// 한국 보고서 불릿 체계의 고정 글리프를 `- `+들여쓰기 목록으로 매핑한다
/// (고정 패스).
///
/// depth1: □/■/ㅁ → `- 본문`, depth2: ○/ㅇ/● → 2칸 `- 본문`,
/// depth3: ·/‧ → 4칸 `- 본문`. 본문이 없는 줄은 건드리지 않는다.
pub(crate) fn restore_bullet_depths(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        let glyph = trimmed.chars().next();
        if let Some(glyph) = glyph {
            if let Some(depth) = rewritten_fixed_depth(glyph) {
                let body = trimmed[glyph.len_utf8()..].trim();
                if !body.is_empty() {
                    out.push(format!("{}- {body}", " ".repeat((depth - 1) * 2)));
                    continue;
                }
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn contextual_bullets_follow_relative_indentation() {
        // 원문 들여쓰기(3칸/6칸)를 문맥 패스가 규약 깊이(2/4칸)로 재작성한다.
        let src = lines(&["□ 상위", "   - 하위", "      - 세부"]);
        assert_eq!(
            restore_bullet_levels(&src.join("\n")),
            lines(&["□ 상위", "  - 하위", "    - 세부"]).join("\n")
        );
        // 고정 패스: □가 `- `로 정규화된다.
        assert_eq!(
            restore_bullet_depths(lines(&["□ 상위", "  - 하위", "    - 세부"])),
            lines(&["- 상위", "  - 하위", "    - 세부"])
        );
    }

    #[test]
    fn deep_raw_indent_normalizes_to_computed_depth() {
        // 원문 들여쓰기가 4칸이어도 앞 불릿이 없으면 2단계다(상대 판정).
        assert_eq!(
            restore_bullet_levels("      - 깊게 들여쓴 첫 항목"),
            "  - 깊게 들여쓴 첫 항목"
        );
    }

    #[test]
    fn blank_lines_keep_context_but_text_breaks_it() {
        // 빈 줄은 맥락 유지 → 세 번째 줄은 3단계.
        assert_eq!(
            restore_bullet_depths(lines(&[
                "□ 상위",
                "",
                "  - 하위",
                "    - 세부"
            ])),
            lines(&["- 상위", "", "  - 하위", "    - 세부"])
        );
        // 불릿이 아닌 본문 줄이 끼면 맥락이 끊긴다 → 다시 2단계.
        let broken = lines(&["□ 상위", "본문 줄", "      - 깊었던 항목"]);
        let restored = restore_bullet_levels(&broken.join("\n"));
        assert!(restored.contains("  - 깊었던 항목"), "{restored}");
    }

    #[test]
    fn circled_numerals_are_preserved_in_body() {
        assert_eq!(
            restore_bullet_levels("① 첫째 항목"),
            "  - ① 첫째 항목"
        );
        // 한글 원문자(㉠)도 열거 기호로 보존한다.
        assert_eq!(restore_bullet_levels("㉠ 가 항목"), "  - ㉠ 가 항목");
        // `-` 뒤에 원문자가 오면 그대로 본문에 남는다.
        assert_eq!(
            restore_bullet_levels("  - ② 둘째"),
            "  - ② 둘째"
        );
    }

    #[test]
    fn note_glyphs_stay_in_body() {
        assert_eq!(restore_bullet_levels("☞ 주석 사항"), "  - ☞ 주석 사항");
    }

    #[test]
    fn fixed_glyphs_map_to_conventional_depths() {
        assert_eq!(
            restore_bullet_depths(lines(&["□ 첫째", "○ 둘째", "· 셋째", "‧ 셋째2"])),
            lines(&["- 첫째", "  - 둘째", "    - 셋째", "    - 셋째2"])
        );
        // 붙어 쓴 고정 글리프도 매핑한다.
        assert_eq!(restore_bullet_depths(lines(&["○붙어쓴항목"])), lines(&["  - 붙어쓴항목"]));
    }

    #[test]
    fn star_and_note_markers_are_left_to_later_passes() {
        // `*`는 각주 별표 패스, `※`는 참고 표기 — 고정 패스가 재작성하지 않는다.
        let src = lines(&["* 별표 항목", "※ 참고 사항", "– 대시 소개줄"]);
        assert_eq!(restore_bullet_depths(src.clone()), src);
    }

    #[test]
    fn visible_sub_items_bump_below_same_indent_glyph() {
        // 같은 들여쓰기의 `- ` 바로 아래에서 온 주석 글리프(☞)는 한 단계 더
        // 깊게 본다(md2hwpx finish_bullets와 같은 판정). 첫 문맥 불릿은
        // 규약상 최하 2단계다.
        let src = lines(&["- 상위", "  - 인접 항목", "  - ☞ 주석 사항"]);
        assert_eq!(
            restore_bullet_levels(&src.join("\n")),
            lines(&["  - 상위", "    - 인접 항목", "    - ☞ 주석 사항"]).join("\n")
        );
    }

    #[test]
    fn indented_asterisk_footnotes_do_not_pollute_bullet_context() {
        // 각주 `* ` 줄은 불릿 항목이 아니므로 맥락에 끼지 않는다 — 뒤 항목의
        // 상대 깊이가 각주 줄 때문에 눌리지 않아야 한다.
        let src = lines(&[
            "  ○ 현황 분석",
            "      * 시범 운영 수치임",
            "    - 인접 동 확대",
        ]);
        let restored = restore_bullet_levels(&src.join("\n"));
        assert!(
            restored.contains("    - 인접 동 확대"),
            "인접은 3단계로 계산되어야 한다: {restored}"
        );
    }

    #[test]
    fn empty_bullets_are_untouched() {
        let src = lines(&["-", "□"]);
        assert_eq!(restore_bullet_levels(&src.join("\n")), "-\n□");
        assert_eq!(restore_bullet_depths(src), lines(&["-", "□"]));
    }
}
