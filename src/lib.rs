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

    #[test]
    fn superscript_run_has_supscript_char_pr() {
        // `^` 윗첨자 run은 hh:supscript를 가진 charPr(빨간색 변형 아님)를
        // 참조해야 한다.
        let bytes = compile("# 제목\n\n이것은^윗첨자 텍스트입니다.");
        let section = body_table_section(&bytes);
        let header = header_section(&bytes);
        assert!(section.contains(">이것은<"), "section:\n{section}");
        // 윗첨자 run이 분리되어 있어야 한다.
        let sup_char_pr_id = {
            let doc = crate::compile::xmltree::parse(section.as_bytes()).unwrap();
            let runs = crate::shared::xmltree::find_all_desc(
                &doc,
                crate::compile::xmltree::HP,
                "run",
            );
            let sup_run = runs
                .iter()
                .find(|r| {
                    crate::shared::xmltree::find_children(r, crate::compile::xmltree::HP, "t")
                        .iter()
                        .map(|t| t.borrow().text_or_empty())
                        .collect::<String>()
                        == "윗첨자"
                })
                .unwrap_or_else(|| panic!("no superscript run in section:\n{section}"));
            let sup_char_pr_id_val = sup_run.borrow().get_attr("charPrIDRef").unwrap();
            sup_char_pr_id_val
        };
        let hdr = crate::compile::xmltree::parse(header.as_bytes()).unwrap();
        let props = crate::shared::xmltree::find_desc(
            &hdr,
            crate::compile::xmltree::HH,
            "charProperties",
        )
        .unwrap();
        let pr = crate::shared::xmltree::find_children(&props, crate::compile::xmltree::HH, "charPr")
            .into_iter()
            .find(|cp| cp.borrow().get_attr("id").as_deref() == Some(sup_char_pr_id.as_str()))
            .unwrap_or_else(|| panic!("charPr {sup_char_pr_id} missing in header:\n{header}"));
        assert!(
            crate::shared::xmltree::find_child(&pr, crate::compile::xmltree::HH, "supscript")
                .is_some(),
            "charPr {sup_char_pr_id} has no supscript:\n{header}"
        );
        assert_ne!(
            pr.borrow().get_attr("textColor").as_deref(),
            Some("#FF0000"),
            "plain superscript must not be the red variant:\n{header}"
        );
    }

    #[test]
    fn superscript_roundtrip_preserves_caret() {
        let md = "# 제목\n\n이것은^윗첨자 텍스트입니다.";
        let back = convert(&compile(md));
        assert!(back.contains("이것은^윗첨자"), "md:\n{back}");
    }

    fn body_table_section(bytes: &[u8]) -> String {
        // 본문 content 표(표지 표가 아닌 colCnt 일치 tbl)가 있는 section0.xml.
        use std::io::Read as _;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("Contents/section0.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        xml
    }

    fn header_section(bytes: &[u8]) -> String {
        use std::io::Read as _;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("Contents/header.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        xml
    }

    /// borderFill 정의의 컴팩트 서명(id 무시, 내용만).
    fn border_fill_signature(def: &str) -> String {
        fn attr(def: &str, tag: &str, name: &str) -> String {
            let marker = format!("<hh:{} ", tag);
            let Some(pos) = def.find(&marker) else {
                return String::new();
            };
            let rest = &def[pos + marker.len()..];
            let end = rest.find("/>").unwrap_or(rest.len());
            let head = &rest[..end];
            let needle = format!("{}=\"", name);
            match head.find(&needle) {
                Some(i) => head[i + needle.len()..]
                    .split('"')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                None => String::new(),
            }
        }
        let brush = if def.contains("winBrush") {
            def.split("faceColor=\"")
                .nth(1)
                .map(|s| s[..s.find('"').unwrap()].to_string())
                .unwrap_or_else(|| "none".to_string())
        } else {
            "none".to_string()
        };
        let top = attr(def, "topBorder", "type");
        let top_color = attr(def, "topBorder", "color");
        let bottom = attr(def, "bottomBorder", "type");
        let bottom_color = attr(def, "bottomBorder", "color");
        format!(
            "L:{} R:{} T:{}/{} B:{}/{} brush:{}",
            attr(def, "leftBorder", "type"),
            attr(def, "rightBorder", "type"),
            top,
            top_color,
            bottom,
            bottom_color,
            brush
        )
    }

    #[test]
    fn two_column_table_uses_native_two_column_prototype() {
        let md = "# 제목\n\n| 항목 | 수치 |\n|---|---|\n| 가 | 1 |\n| 나 | 2 |\n| 다 | 3 |";
        let bytes = compile(md);
        let xml = body_table_section(&bytes);
        let tbl_start = xml.find("colCnt=\"2\"").expect("2열 본문 표가 없음");
        let start = xml[..tbl_start]
            .rfind("<hp:tbl ")
            .expect("tbl 시작 태그 없음");
        let end = xml[start..].find("</hp:tbl>").unwrap() + start;
        let table = &xml[start..end];
        let trs: Vec<usize> = table.match_indices("<hp:tr>").map(|(i, _)| i).collect();
        assert_eq!(trs.len(), 4, "헤더 + 데이터 3행이어야 한다: {table}");

        let fill_refs = |row: &str| -> Vec<String> {
            let mut out = Vec::new();
            let mut rest = row;
            while let Some(i) = rest.find("borderFillIDRef=\"") {
                let after = &rest[i + "borderFillIDRef=\"".len()..];
                let v: String = after.chars().take_while(|c| *c != '"').collect();
                out.push(v.clone());
                rest = &after[v.len()..];
            }
            out
        };
        let rows: Vec<Vec<String>> = trs
            .iter()
            .enumerate()
            .map(|(k, start)| {
                let end = trs.get(k + 1).copied().unwrap_or(table.len());
                fill_refs(&table[*start..end])
            })
            .collect();
        let last = rows[3].clone();
        let rows = &rows[..3];
        // 중간 원형: tr1과 tr2는 같은 쌍.
        assert_eq!(
            rows[1], rows[2],
            "중간 데이터 행들의 borderFill 쌍이 같아야 한다: {:?}",
            rows
        );
        // 마지막 행은 원형의 SOLID 행: tr1과 다른 쌍.
        assert_ne!(
            rows[1], last,
            "마지막 행은 원형의 마지막 SOLID 행을 써야 한다: tr1={:?} tr3={:?}",
            rows[1], last
        );
        // 헤더 행의 쌍은 중간 쌍과 다르다.
        assert_ne!(rows[0], rows[1], "헤더 행 쌍이 중간 쌍과 같다: {:?}", rows);

        // 네이티브 table-2 원형의 borderFill 서명 집합(헤더 12/8, 중간 14/9,
        // SOLID 마지막 7/13 — hp assets/block/table-2.hwpx에서 온 값).
        let native_signatures: Vec<String> = vec![
            "L:NONE R:DASH T:SOLID/#808080 B:SOLID/#808080 brush:#DFE6F7", // 헤더 첫 셀
            "L:DASH R:NONE T:SOLID/#808080 B:SOLID/#808080 brush:#DFE6F7", // 헤더 둘째 셀
            "L:NONE R:DASH T:SOLID/#808080 B:DASH/#808080 brush:none",     // 중간 첫 셀
            "L:DASH R:NONE T:SOLID/#808080 B:DASH/#808080 brush:none",     // 중간 둘째 셀
            "L:NONE R:DASH T:SOLID/#808080 B:SOLID/#000000 brush:none",    // 마지막 첫 셀
            "L:DASH R:NONE T:SOLID/#808080 B:SOLID/#000000 brush:none",    // 마지막 둘째 셀
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let header = header_section(&bytes);
        let defs: std::collections::HashMap<String, String> = header
            .match_indices("<hh:borderFill ")
            .map(|(i, _)| {
                let rest = &header[i..];
                let end = rest.find("</hh:borderFill>").unwrap();
                let def = &rest[..end + "</hh:borderFill>".len()];
                let id: String = def
                    .split("id=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .to_string();
                (id, border_fill_signature(def))
            })
            .collect();
        let used: Vec<String> = rows
            .iter()
            .chain(std::iter::once(&last))
            .flatten()
            .map(|id| defs[id].clone())
            .collect();
        let mut used_sorted = used.clone();
        used_sorted.sort();
        used_sorted.dedup();
        let mut native_sorted = native_signatures.clone();
        native_sorted.sort();
        assert_eq!(
            used_sorted, native_sorted,
            "본문 2열 표는 네이티브 table-2 원형의 borderFill을 써야 한다: {:?}",
            used
        );

        // 왕복 보존(커버리지 목적 — RED 주장은 위 네이티브 원형 단언에 근거한다).
        let md2 = convert(&compile(md));
        let data_rows = md2
            .lines()
            .filter(|l| l.starts_with("| 가") || l.starts_with("| 나") || l.starts_with("| 다"))
            .count();
        assert_eq!(data_rows, 3, "파이프 표 데이터 행 3개가 보존되어야 한다:\n{md2}");
    }

    #[test]
    fn three_column_table_keeps_default_prototype_and_solid_last_row() {
        // coverage-only: 3열 입력은 기본 "table" 원형 경로와 슬라이스 L의
        // 마지막 행 SOLID 규칙을 그대로 유지해야 한다(table-2 변형 추가와
        // 무관).
        let md = "# 제목\n\n| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n| 7 | 8 | 9 |";
        let bytes = compile(md);
        let xml = body_table_section(&bytes);
        let tbl_start = xml.find("colCnt=\"3\"").expect("3열 본문 표가 없음");
        let start = xml[..tbl_start].rfind("<hp:tbl ").unwrap();
        let end = xml[start..].find("</hp:tbl>").unwrap() + start;
        let table = &xml[start..end];
        let trs: Vec<usize> = table.match_indices("<hp:tr>").map(|(i, _)| i).collect();
        assert_eq!(trs.len(), 4, "헤더 + 데이터 3행이어야 한다");
        let refs_of = |row: &str| -> Vec<String> {
            let mut out = Vec::new();
            let mut rest = row;
            while let Some(i) = rest.find("borderFillIDRef=\"") {
                let after = &rest[i + "borderFillIDRef=\"".len()..];
                let v: String = after.chars().take_while(|c| *c != '"').collect();
                out.push(v.clone());
                rest = &after[v.len()..];
            }
            out
        };
        let rows: Vec<Vec<String>> = trs
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let e = trs.get(k + 1).copied().unwrap_or(table.len());
                refs_of(&table[*s..e])
            })
            .collect();
        assert_eq!(rows[1], rows[2], "중간 행들은 같은 borderFill 쌍");
        assert_ne!(rows[1], rows[3], "마지막 행은 SOLID 원형 행");
        assert_ne!(rows[0], rows[1], "헤더 행 쌍은 중간 쌍과 다름");
        assert_eq!(rows[0].len() + rows[1].len() + rows[3].len(), 9, "3열");
    }
}
