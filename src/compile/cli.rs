use std::path::{Path, PathBuf};

pub struct Args {
    pub source: PathBuf,
    pub template: PathBuf,
    pub output: Option<PathBuf>,
    pub variants: Vec<(String, String)>,
}

pub const USAGE: &str = "\
usage: md2hwpx [source] [template] [output]

options:
  --variant CATEGORY=NAME   [[CATEGORY:NAME]] variant selection (repeatable)
  -v, --version             show build date
  -h, --help                show help";

fn usage_error(message: &str) -> ! {
    eprintln!("{USAGE}\n\nerror: {message}");
    std::process::exit(2);
}

fn push_variant(variants: &mut Vec<(String, String)>, value: &str) {
    let Some((category, name)) = value.split_once('=') else {
        usage_error(&format!("--variant must be CATEGORY=NAME: {value}"));
    };
    if category.is_empty() || name.is_empty() {
        usage_error(&format!("--variant must be CATEGORY=NAME: {value}"));
    }
    variants.push((category.to_string(), name.to_string()));
}

/// argparse 기본값(source.txt/template.hwpx/compiled.hwpx)과 --variant
/// 반복 옵션을 흉내 낸 인자 해석. 오류 시 사용법을 출력하고 종료 코드 2.
pub fn parse_args() -> Args {
    let mut positional: Vec<String> = Vec::new();
    let mut variants: Vec<(String, String)> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("md2hwpx {} (built {})", env!("CARGO_PKG_VERSION"), env!("BUILD_DATE"));
                std::process::exit(0);
            }
            "--variant" => {
                let value = it.next().unwrap_or_else(|| {
                    usage_error("--variant must be CATEGORY=NAME: (no value)");
                });
                push_variant(&mut variants, &value);
            }
            a if a.starts_with("--variant=") => {
                push_variant(&mut variants, &a["--variant=".len()..]);
            }
            a if a.starts_with('-') && a.len() > 1 => {
                usage_error(&format!("unrecognized option: {a}"));
            }
            _ => positional.push(arg),
        }
    }
    if positional.len() > 3 {
        usage_error(&format!("too many arguments: {}", positional[3]));
    }
    let defaults = ["source.txt", "template.hwpx"];
    while positional.len() < 2 {
        positional.push(defaults[positional.len()].to_string());
    }
    Args {
        source: PathBuf::from(&positional[0]),
        template: PathBuf::from(&positional[1]),
        output: positional.get(2).map(PathBuf::from),
        variants,
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

/// 출력이 source/template와 같은 파일(같은 경로)인지 검사한다.
pub fn output_collides(output: &Path, source: &Path, template: &Path) -> bool {
    normalize_path(output) == normalize_path(source) || normalize_path(output) == normalize_path(template)
}

/// 파일명에 쓸 수 없는 문자를 제거하고 앞뒤 공백과 마침표를 잘라낸다.
fn sanitize_file_stem(title: &str) -> String {
    title
        .chars()
        .filter(|c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

/// 제목을 인식했을 때의 기본 출력 파일명. "yymmdd 제목 hhmmss.hwpx"(로컬
/// 시간 기준 지금, 날짜 6자리 + 시간 6자리).
pub fn default_output_name(title: &str) -> String {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let stem = sanitize_file_stem(title);
    if stem.is_empty() {
        return "compiled.hwpx".to_string();
    }
    format!(
        "{:02}{:02}{:02} {} {:02}{:02}{:02}.hwpx",
        now.year() % 100,
        now.month(),
        now.day(),
        stem,
        now.hour(),
        now.minute(),
        now.second()
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
    fn default_output_name_appends_hhmmss() {
        use regex::Regex;
        let name = default_output_name(" 검토(안) ");
        let re = Regex::new(r"^\d{6} 검토\(안\) \d{6}\.hwpx$").unwrap();
        assert!(re.is_match(&name), "예상 형식 아님: {name}");
    }

    #[test]
    fn default_output_name_falls_back_when_title_is_empty() {
        assert_eq!(default_output_name(" :*? "), "compiled.hwpx");
    }

    #[test]
    fn output_collides_detects_same_file() {
        let dir = std::env::temp_dir();
        assert!(output_collides(&dir.join("a.txt"), &dir.join("a.txt"), &dir.join("b.txt")));
        assert!(!output_collides(&dir.join("c.hwpx"), &dir.join("a.txt"), &dir.join("b.txt")));
    }
}
