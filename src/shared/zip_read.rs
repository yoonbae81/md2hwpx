//! md2hwpx·hwpx2md 양쪽 hwpx.rs에서 바이트 동일이던 zip 읽기 헬퍼의
//! 단일 출처: 패키지 상한 검사, DTD/엔티티 선언 거부, section 파일 정렬.
//! 쓰기 경로(패키지 재작성)와 검증 경로는 각 크레이트에 남아 있다.

use std::io::Read;

use zip::read::ZipArchive;

use super::error::{err, AppError, Result};

pub const HWPX_MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

pub fn reject_dtd(data: &[u8], label: &str) -> Result<()> {
    // ElementTree에는 엔티티 확장 방지 장치가 없어 DTD 선언을 거부한다.
    if contains_sub(data, b"<!DOCTYPE") || contains_sub(data, b"<!ENTITY") {
        return err(format!("{label} contains a DTD/entity declaration; refusing to process it"));
    }
    Ok(())
}

pub fn contains_sub(haystack: &[u8], needle: &[u8]) -> bool {
    // needle은 항상 '<'로 시작하므로 그 바이트만 건너뛰며 비교한다
    // (전 구간 나이브 비교보다 XML에서 수십 배 빠르다).
    let Some(first) = needle.first() else { return true };
    let mut start = 0usize;
    while let Some(pos) = haystack[start..].iter().position(|&b| b == *first) {
        let at = start + pos;
        if haystack[at..].starts_with(needle) {
            return true;
        }
        start = at + 1;
    }
    false
}

pub fn check_package_size<R: Read + std::io::Seek>(
    package: &mut ZipArchive<R>,
    label: &str,
) -> Result<()> {
    // 선언된 압축 해제 크기의 합으로 상한을 검사한다. 항목을 읽다가
    // 실패하면 총량을 알 수 없으므로 오류로 전파한다.
    let mut total: u64 = 0;
    for i in 0..package.len() {
        total += package
            .by_index(i)
            .map_err(|e| AppError(format!("{label}: {e}")))?
            .size();
    }
    if total > HWPX_MAX_PACKAGE_BYTES {
        return err(format!(
            "uncompressed size of {label} ({} bytes) is too large",
            thousands(total)
        ));
    }
    Ok(())
}

fn section_num_re() -> &'static regex::Regex {
    use std::sync::LazyLock;
    static RE: std::sync::LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"section(\d+)\.xml$").unwrap());
    &RE
}

/// `_section_sort_key` — section10.xml이 section2.xml보다 사전식으로
/// 앞서는 문제를 피한다. 파일명에서 절 번호를 정수로 추출해 정렬한다 —
/// 번호를 못 찾으면(예상치 못한 파일명) 정상 인식된 절들 뒤로 보내되
/// 원래 문자열 순서는 보존한다.
pub fn section_sort_key(name: &str) -> (u8, u64, String) {
    match section_num_re()
        .captures(name)
        .and_then(|c| c[1].parse::<u64>().ok())
    {
        Some(num) => (0, num, name.to_string()),
        None => (1, 0, name.to_string()),
    }
}

/// zip 항목 이름 중 `Contents/section*` 파일을 절 번호 순으로 돌려준다.
pub fn section_files(names: &[String]) -> Vec<String> {
    let mut filtered: Vec<String> = names
        .iter()
        .filter(|n| n.starts_with("Contents/section"))
        .cloned()
        .collect();
    filtered.sort_by(|a, b| section_sort_key(a).cmp(&section_sort_key(b)));
    filtered
}

/// 최소 HWPX 패키지 픽스처. hwpx2md/md2hwpx의 테스트 모듈이
/// `common::zip_read::test_fixtures::fixture_hwpx`로 가져다 쓴다.
///
/// cfg(test) 게이트를 쓰지 않는다 — common은 두 크레이트의 일반 의존성이라
/// 의존 크레이트를 테스트 빌드할 때 cfg(test) 심볼이 존재하지 않는다.
/// 미참조 코드는 릴리스 빌드에서 LTO로 제거된다.
#[doc(hidden)]
pub mod test_fixtures {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    /// 최소 HWPX 패키지를 메모리에서 만든다(바이너리 픽스처 커밋 회피).
    /// `sections`는 (항목 이름, XML 내용) 쌍이다.
    pub fn fixture_hwpx(sections: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut buf));
            let stored =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file("mimetype", stored).unwrap();
            zw.write_all(b"application/hwp+zip").unwrap();
            zw.start_file("version.xml", deflated).unwrap();
            zw.write_all(b"1.0").unwrap();
            zw.start_file("Contents/content.hpf", deflated).unwrap();
            zw.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?><opf:Package/>").unwrap();
            zw.start_file("Contents/header.xml", deflated).unwrap();
            zw.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?><hh:header/>").unwrap();
            for (name, xml) in sections {
                zw.start_file(name, deflated).unwrap();
                zw.write_all(xml.as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_section_files_numerically() {
        // section10.xml이 section2.xml보다 앞서면 문서 순서가 깨진다.
        let names = [
            "Contents/header.xml".to_string(),
            "Contents/section10.xml".to_string(),
            "Contents/section2.xml".to_string(),
            "Contents/section0.xml".to_string(),
            "Contents/sectionX.xml".to_string(),
        ];
        assert_eq!(
            section_files(&names),
            vec![
                "Contents/section0.xml".to_string(),
                "Contents/section2.xml".to_string(),
                "Contents/section10.xml".to_string(),
                "Contents/sectionX.xml".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_dtd_and_entity_declarations() {
        assert!(reject_dtd(b"<!DOCTYPE html>", "x").is_err());
        assert!(reject_dtd(b"<a><!ENTITY x></a>", "x").is_err());
        assert!(reject_dtd(b"<hp:hs xmlns:hp=\"urn\"></hp:hs>", "x").is_ok());
    }

    #[test]
    fn contains_sub_matches_and_misses() {
        assert!(contains_sub(b"<a><!DOCTYPE", b"<!DOCTYPE"));
        assert!(!contains_sub(b"<a> plain doc", b"<!DOCTYPE"));
        assert!(!contains_sub(b"", b"<!DOCTYPE"));
    }
}
