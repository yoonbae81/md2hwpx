//! exe용 래퍼. 파일 I/O와 종료 코드 규약만 담당하고 변환 자체는
//! lib(hwpx2md)의 인메모리 파이프라인에 위임한다.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use hwpx::shared::error::{create_error, err, io_context, open_error, Result};
use hwpx::parse::cli;
use hwpx::parse::report::Report;

fn run() -> Result<Report> {
    let args = cli::parse_args();
    let input_bytes = fs::read(&args.input).map_err(|e| open_error(&args.input, e))?;
    let label = args.input.display().to_string();
    let (markdown, report) = hwpx::parse::hwpx_to_markdown_bytes(&input_bytes, &label)?;
    let output: PathBuf = args
        .output
        .clone()
        .unwrap_or_else(|| cli::default_output_name(&args.input));
    if cli::output_collides(&output, &args.input) {
        return err("output path must differ from the input file.");
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                io_context(format!("cannot create folder {}", parent.display()), e)
            })?;
        }
    }
    fs::write(&output, &markdown).map_err(|e| create_error(&output, e))?;
    Ok(report)
}

fn main() -> ExitCode {
    match run() {
        Err(e) => {
            eprintln!("error: {e}");
            // 처리/I-O 실패는 1. 사용법/인자 오류(2)는 cli::parse_args가 담당.
            ExitCode::from(1)
        }
        Ok(report) => {
            let json = serde_json::to_string_pretty(&report).expect("JSON serialization failed");
            println!("{json}");
            // 경고는 stdout JSON Report에 그대로 남고 종료 코드에는 영향을
            // 주지 않는다(0=성공, 1=처리 실패는 위의 오류 분기).
            ExitCode::SUCCESS
        }
    }
}
