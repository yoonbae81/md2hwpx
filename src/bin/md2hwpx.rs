//! exe용 래퍼. 파일 I/O와 종료 코드 규약만 담당하고 변환 자체는
//! lib(md2hwpx)의 인메모리 파이프라인에 위임한다.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hwpx::shared::error::{create_error, err, io_context, open_error, AppError, Result};
use hwpx::compile::cli;
use hwpx::compile::Report;

/// UTF-8(BOM 허용) 소스 파일을 문자열로 읽는다.
fn read_source_text(path: &Path) -> Result<String> {
    let mut bytes = fs::read(path).map_err(|e| open_error(path, e))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError(format!("{}: input is not valid UTF-8.", path.display())))
}

fn run() -> Result<Report> {
    let args = cli::parse_args();
    let text = read_source_text(&args.source)?;
    let template_bytes = fs::read(&args.template).map_err(|e| open_error(&args.template, e))?;
    let (bytes, mut report) = hwpx::compile::compile_hwpx_labeled(
        &text,
        &template_bytes,
        &args.variants,
        &args.source.display().to_string(),
        &args.template.display().to_string(),
    )?;
    let output: PathBuf = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(cli::default_output_name(&report.title)));
    if cli::output_collides(&output, &args.source, &args.template) {
        return err("output path must differ from source and template.");
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                io_context(format!("cannot create folder {}", parent.display()), e)
            })?;
        }
    }
    fs::write(&output, &bytes).map_err(|e| create_error(&output, e))?;
    report.output = output.display().to_string();
    Ok(report)
}

fn main() -> ExitCode {
    match run() {
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
        Ok(report) => {
            let json = serde_json::to_string_pretty(&report).expect("JSON serialization failed");
            println!("{json}");
            if report.validation.errors.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
    }
}
