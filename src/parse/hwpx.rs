//! HWPX 패키지(zip) 읽기 경로. 공용 zip 읽기 헬퍼(상한·DTD 거부·section
//! 정렬)는 common::zip_read에서 오고, 여기서는 `Contents/section*.xml`을
//! 읽어 파싱하는 것만 담당한다. 쓰기 경로는 없다(hwpx2md는 패키지를
//! 수정하지 않는다).
//!
//! header.xml를 일부러 읽지 않는다 — Python 참조 구현과 같은 설계로,
//! 문서 구조는 스타일/charPr 메타데이터가 아니라 텍스트 내용 패턴으로
//! 복원한다(v1 의도 결정사항이며 누락이 아니다).

use std::io::{Cursor, Read};

use crate::shared::zip_read::{check_package_size, reject_dtd, section_files};
use zip::read::ZipArchive;

use crate::shared::error::{AppError, Result};
use crate::shared::xmltree::{parse, NodeRef};

/// zip과 엔티티 안전 검사를 거쳐 `Contents/section*.xml`을 번호 순서대로
/// 파싱해 트리를 모은다. `label`은 오류 메시지에 쓰이는 입력 출처이다.
pub fn read_sections(bytes: &[u8], label: &str) -> Result<Vec<NodeRef>> {
    let mut package = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError(format!("{label}: {e}")))?;
    check_package_size(&mut package, label)?;
    let names: Vec<String> = package.file_names().map(str::to_string).collect();
    let mut roots = Vec::new();
    for name in section_files(&names) {
        let mut f = package
            .by_name(&name)
            .map_err(|e| AppError(format!("{label}: {e}")))?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        reject_dtd(&data, &name)?;
        let root = parse(&data).map_err(|e| AppError(format!("malformed XML in {name}: {e}")))?;
        roots.push(root);
    }
    Ok(roots)
}

#[cfg(test)]
pub(crate) use crate::shared::zip_read::test_fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sections_in_order_from_package() {
        let hs = |text: &str| {
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p></hp:hs>"#
            )
        };
        let bytes = test_fixtures::fixture_hwpx(&[
            ("Contents/section0.xml", &hs("첫절")),
            ("Contents/section1.xml", &hs("둘째절")),
        ]);
        let roots = read_sections(&bytes, "sample.hwpx").unwrap();
        assert_eq!(roots.len(), 2);
        let text: String = crate::shared::xmltree::text_nodes(&roots[1])
            .iter()
            .map(|n| n.borrow().text_or_empty())
            .collect();
        assert_eq!(text, "둘째절");
    }

    #[test]
    fn missing_sections_yields_empty_vec() {
        let bytes = test_fixtures::fixture_hwpx(&[]);
        let roots = read_sections(&bytes, "empty.hwpx").unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn truncated_zip_is_an_error() {
        let mut bytes = test_fixtures::fixture_hwpx(&[("Contents/section0.xml", "<a/>")]);
        bytes.truncate(bytes.len() / 2);
        assert!(read_sections(&bytes, "broken.hwpx").is_err());
    }
}
