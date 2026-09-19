use std::path::{Path, PathBuf};

use crate::shared::filename::sanitize_file_stem;

pub struct Args {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
}

pub const USAGE: &str = "\
usage: hwpx2md <input.hwpx> [output.md]

options:
  -v, --version   show build date
  -h, --help      show help";

fn usage_error(message: &str) -> ! {
    eprintln!("{USAGE}\n\nerror: {message}");
    std::process::exit(2);
}

/// 인자 해석. 입력 파일은 필수 positional이고(기본 파일명 추측 없음),
/// 생략하면 사용법을 출력하고 종료 코드 2로 끝난다. 출력 파일을 생략하면
/// main이 `default_output_name`(<입력줄기>.md)으로 정한다.
pub fn parse_args() -> Args {
    let mut positional: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!(
                    "hwpx2md {} (built {})",
                    env!("CARGO_PKG_VERSION"),
                    env!("BUILD_DATE")
                );
                std::process::exit(0);
            }
            a if a.starts_with('-') && a.len() > 1 => {
                usage_error(&format!("unrecognized option: {a}"));
            }
            _ => positional.push(arg),
        }
    }
    if positional.is_empty() {
        usage_error("missing required input file");
    }
    if positional.len() > 2 {
        usage_error(&format!("too many arguments: {}", positional[2]));
    }
    Args {
        input: PathBuf::from(&positional[0]),
        output: positional.get(1).map(PathBuf::from),
    }
}

fn normalize_path(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
        if let Ok(cp) = parent.canonicalize() {
            return cp.join(name);
        }
    }
    p.to_path_buf()
}

/// 출력이 입력 파일과 같은 파일(같은 경로)인지 검사한다.
pub fn output_collides(output: &Path, input: &Path) -> bool {
    normalize_path(output) == normalize_path(input)
}

/// 출력 파일명 기본값: 현재 디렉터리의 <입력줄기>.md. 줄기는 md2hwpx와
/// 같은 규칙(shared::filename — 금지 문자·제어·zero-width/bidi 제거)으로
/// 정리하고, 정리 뒤 비면 "output.md"로 돌아간다.
pub fn default_output_name(input: &Path) -> PathBuf {
    let raw_stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let stem = sanitize_file_stem(&raw_stem);
    if stem.is_empty() {
        return PathBuf::from("output.md");
    }
    PathBuf::from(format!("{stem}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_output_name_replaces_extension_with_md() {
        assert_eq!(
            default_output_name(Path::new("workspace/보고서 초안.hwpx")),
            PathBuf::from("보고서 초안.md")
        );
    }

    #[test]
    fn default_output_name_strips_zero_width_and_bidi() {
        // 입력 파일명에 끼어든 비가시 유니코드가 줄기에 그대로 흘러들지 않는다.
        assert_eq!(
            default_output_name(Path::new("\u{202E}보고서\u{200B}\u{2066}.hwpx")),
            PathBuf::from("보고서.md")
        );
        assert_eq!(default_output_name(Path::new("\u{200D}:*?.hwpx")), PathBuf::from("output.md"));
    }

    #[test]
    fn output_collides_detects_same_file() {
        let dir = std::env::temp_dir();
        assert!(output_collides(&dir.join("a.md"), &dir.join("a.md")));
        assert!(!output_collides(&dir.join("a.md"), &dir.join("a.hwpx")));
    }
}
