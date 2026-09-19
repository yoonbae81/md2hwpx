//! 표지 날짜 줄 제거.
//!
//! md2hwpx 템플릿은 표지 날짜를 컴파일 시점에 오늘 날짜로 다시 찍는다
//! (patterns.rs의 TEMPLATE_DATE_RE와 같은 패턴, fwSpace 흡수 형태의 공백
//! 허용 포함). HWPX → 마크다운에서 이 줄을 본문으로 내보내면 재컴파일 때
//! 표지 날짜와 이중으로 나타나고, 불릿 아닌 본문 줄은 depth1 블록이 되는
//! md2hwpx 규약상 `- ’26.9.8(화), …` 불릿으로 변질된다.
//!
//! 따라서 문서 맨 앞(`# ` 제목 바로 다음)의 템플릿 날짜 패턴 줄 한 줄만
//! 제거한다. 규칙 순서상 소제목 승격(heading.rs) 다음에 실행되어야 한다 —
//! 승격 전에는 표지 제목이 아직 `| 제목 |` 파이프 표 형태라 `# ` 게이트를
//! 통과하지 않는다. 그 외 위치의 날짜 문양은 본문 내용으로 보고 건드리지
//! 않는다 — 실제 문서에서 날짜 줄이 내용일 수 있으므로 판정 범위를 표지로
//! 한정한다.

use std::sync::LazyLock;

use regex::Regex;

static TEMPLATE_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[’']\d{2}\.\s*\d{1,2}\.\s*\d{1,2}\s*\([월화수목금토일]\)").unwrap()
});

/// 첫 비공백 줄이 `# ` 제목일 때만 그다음 비공백 줄을 표지 날짜 후보로
/// 보고, 템플릿 날짜 패턴에 맞으면 그 한 줄을 제거한다. md2hwpx 컴파일
/// 산출물은 항상 표지 제목이 문서 맨 앞에 오므로, 제목 없이 날짜로 시작하는
/// 문서의 첫 줄은 표지가 아니라 내용으로 본다.
pub(crate) fn drop_cover_date_line(lines: Vec<String>) -> Vec<String> {
    let first_nonblank = lines.iter().position(|l| !l.trim().is_empty());
    let Some(start) = first_nonblank else {
        return lines;
    };
    if !lines[start].starts_with("# ") {
        return lines;
    }
    let candidate = ((start + 1)..lines.len()).find(|&i| !lines[i].trim().is_empty());
    let Some(di) = candidate else {
        return lines;
    };
    if TEMPLATE_DATE.is_match(lines[di].trim()) {
        let mut out = lines;
        out.remove(di);
        return out;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cover_date_after_title_is_dropped() {
        let src = lines(&["# 제목", "", "’26.9.8(화), 작성자 (☎ 전화번호)", "", "본문"]);
        assert_eq!(
            drop_cover_date_line(src),
            lines(&["# 제목", "", "", "본문"])
        );
    }

    #[test]
    fn fwspace_absorbed_date_variant_is_dropped() {
        let src = lines(&["# 제목", "’26. 9. 8 (화), 작성자"]);
        assert_eq!(drop_cover_date_line(src), lines(&["# 제목"]));
    }

    #[test]
    fn dates_outside_cover_position_are_kept() {
        // 제목과 다른 줄 사이에 날짜가 있으면 본문 내용으로 본다.
        let src = lines(&["# 제목", "먼저 온 문단", "’26.9.8(화) 확인"]);
        assert_eq!(drop_cover_date_line(src.clone()), src);
        // 제목이 없으면 첫 줄이 날짜여도 표지가 아니다(제목 게이트 미통과).
        let no_title = lines(&["’26.9.8(화) 날짜", "# 제목"]);
        assert_eq!(drop_cover_date_line(no_title.clone()), no_title);
    }

    #[test]
    fn non_date_first_line_is_untouched() {
        let src = lines(&["# 제목", "일반 문단"]);
        assert_eq!(drop_cover_date_line(src.clone()), src);
    }
}
