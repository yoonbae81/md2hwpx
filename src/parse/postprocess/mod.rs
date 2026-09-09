//! 컨버터 출력 마크다운의 공용 후처리(한국형 문서 관습의 역변환) 드라이버.
//! retriever markdown_postprocess.py의 규칙 순서를 모듈별로 나눠 담는다:
//!
//!   1. 붙임 표지 표 → `[[붙임]]` 명령 (heading.rs)
//!   2. 장표 제목 바 표 → 번호 단계 수 헤딩 (heading.rs)
//!   3. 소제목 1×1 표 → 마크다운 헤딩 (heading.rs)
//!   4. 표지 날짜 줄 제거 (cover.rs) — 승격된 `# ` 제목 기준, 템플릿이
//!      재생성하므로 이중 방지
//!   5. 박스/하이라이트 컴포넌트 복원 (box.rs)
//!   6. 번호 소제목 → 헤딩 승격 (heading.rs)
//!   7. 깊이별 불릿 → 마크다운 1~3단계 목록, 두 패스 (bullets.rs)
//!   8. 각주 asterisk 들여쓰기 (footnote.rs)
//!
//! 규칙 순서는 계약이다 — 붙임 규칙이 다른 표 규칙보다 먼저여야 3열 표지
//! 표가 잘못 처리되지 않고, 표지 날짜 제거는 승격된 `# ` 제목이 있어야
//! 판정할 수 있으며, 문맥 불릿 패스가 고정 글리프 매핑 직전에 실행되고,
//! 각주 asterisk 들여쓰기는 마지막에 판정된다.

mod bullets;
pub(crate) mod r#box;
mod cover;
pub(crate) mod footnote;
pub(crate) mod heading;

/// 코드 펜스(```로 감싼 구간) 내부는 그대로 두고 나머지에만 규칙을 적용한다.
pub(crate) fn apply_outside_code(
    lines: Vec<String>,
    rule: fn(Vec<String>) -> Vec<String>,
) -> Vec<String> {
    const CODE_FENCE: &str = "```";
    let mut out: Vec<String> = Vec::new();
    let mut segment: Vec<String> = Vec::new();
    let mut in_code = false;
    for line in lines {
        if line.trim().starts_with(CODE_FENCE) {
            if !segment.is_empty() {
                out.extend(rule(std::mem::take(&mut segment)));
            }
            out.push(line);
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(line);
        } else {
            segment.push(line);
        }
    }
    if !segment.is_empty() {
        out.extend(rule(segment));
    }
    out
}

/// 컨버터 출력 마크다운에 규약 복원 규칙을 순서대로 적용한다.
///
/// 코드 블록 내부는 보존한다. 문맥 불릿 패스가 고정 글리프 매핑 직전에
/// 실행되고, 각주 asterisk 들여쓰기는 마지막에 판정한다.
pub fn postprocess_markdown(markdown: &str) -> String {
    let lines: Vec<String> = markdown.split('\n').map(str::to_string).collect();
    let mut lines = lines;
    for rule in [
        heading::attachment_table_to_heading,
        heading::title_bar_table_to_heading,
        heading::subtitle_table_to_heading,
        cover::drop_cover_date_line,
        r#box::restore_boxes,
        heading::promote_numbered_headings,
    ] {
        lines = apply_outside_code(lines, rule);
    }
    // 불릿 패스: 문맥 추적(들여쓰기 기반) → 고정 글리프 매핑 순서가 규약이다.
    // Python 참조와 달리 문맥 패스에도 코드 펜스 가드를 적용한다(펜스 안의
    // 불릿 모양 줄은 훼손 대상이 아니다).
    let restored = bullets::restore_bullet_levels(&lines.join("\n"));
    let lines: Vec<String> = restored.split('\n').map(str::to_string).collect();
    let lines = apply_outside_code(lines, bullets::restore_bullet_depths);
    let lines = apply_outside_code(lines, footnote::indent_footnote_asterisks);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_fence_blocks_are_untouched_by_the_whole_pipeline() {
        let markdown = "\
[박스] 실제 박스
□ 요약 항목
```
[박스] 코드 속 예시
□ 유지되어야 하는 줄
  - 이것도 유지
```
* 뒤 문단의 별표";
        let out = postprocess_markdown(markdown);
        let mut it = out.split('\n');
        assert_eq!(it.next().unwrap(), "[[박스]] 실제 박스");
        assert_eq!(it.next().unwrap(), "- 요약 항목");
        assert_eq!(it.next().unwrap(), "```");
        assert_eq!(it.next().unwrap(), "[박스] 코드 속 예시");
        assert_eq!(it.next().unwrap(), "□ 유지되어야 하는 줄");
        assert_eq!(it.next().unwrap(), "  - 이것도 유지");
        assert_eq!(it.next().unwrap(), "```");
        assert_eq!(it.next().unwrap(), "* 뒤 문단의 별표");
    }

    #[test]
    fn rule_order_promotes_subtitle_inside_doc_start_only() {
        // 문서 맨 앞 블록 판정은 이전 규칙(표→헤딩)의 산출에 의존한다 —
        // 순서가 바뀌면 `# ` 표지 제목이 `## `로 잘못 나온다.
        let markdown = "| 표지 제목 |\n| --- |\n본문";
        assert!(postprocess_markdown(markdown).starts_with("# 표지 제목"));
    }
}
