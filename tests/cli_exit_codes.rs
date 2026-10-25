//! CLI 종료 코드 규약 통합 테스트.
//!
//! 규약: 0=성공(경고 포함 — 경고는 stdout JSON `Report`에 그대로 남는다),
//! 1=처리/검증/I-O 실패, 2=사용법/인자 오류. 모든 케이스는 `CARGO_BIN_EXE_*`
//! 자식 프로세스로 실제 실행 파일을 구동하고 종료 코드를 단정한다. 외부
//! 의존 없이 std만 쓰고, 픽스처는 테스트마다 고유한 `std::env::temp_dir()`
//! 하위 디렉터리(TMPDIR 존중)에 만들고 Drop에서 지운다.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// 테스트별 고유 임시 디렉터리 보관자. PID+원자 카운터로 병렬 테스트 사이
/// 충돌이 없는 경로를 만들고, 테스트가 끝나면 통째로 지운다.
struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "md2hwpx-exit-codes-{}-{}-{tag}",
            std::process::id(),
            seq
        ));
        std::fs::create_dir_all(&path).expect("임시 디렉터리 생성 실패");
        TempDirGuard { path }
    }

    fn child(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 처리(I-O) 실패 공통 단정: 종료 코드 1, stderr에 `error:`와 못 연 파일,
/// 성공 산출물 없음(stdout에 리포트 없음, 출력 파일 미생성).
fn assert_processing_failure(result: &Output, missing: &PathBuf, output: &PathBuf, bin: &str) {
    let code = result.status.code();
    assert_eq!(
        code,
        Some(1),
        "{bin}: 입력 파일 없음은 종료 코드 1이어야 한다(실제 {code:?})"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("error:"),
        "{bin}: stderr에 error: 가 있어야 한다: {stderr:?}"
    );
    assert!(
        stderr.contains("cannot open"),
        "{bin}: 어느 파일을 못 열었는지 stderr가 밝혀야 한다: {stderr:?}"
    );
    assert!(
        stderr.contains(missing.to_string_lossy().as_ref()),
        "{bin}: 누락된 입력 경로가 stderr에 있어야 한다: {stderr:?}"
    );
    assert!(
        result.stdout.is_empty(),
        "{bin}: 실패 시 stdout에 리포트를 출력하지 않는다"
    );
    assert!(!output.exists(), "{bin}: 실패 시 출력 파일을 만들지 않는다");
}

#[test]
fn md2hwpx_missing_input_exits_1() {
    let guard = TempDirGuard::new("missing-input");
    let source = guard.child("source.md");
    let template = guard.child("template.hwpx");
    let output = guard.child("out.hwpx");

    let result = Command::new(env!("CARGO_BIN_EXE_md2hwpx"))
        .arg(&source)
        .arg(&template)
        .arg(&output)
        .output()
        .expect("md2hwpx 실행 실패");
    assert_processing_failure(&result, &source, &output, "md2hwpx");
}

#[test]
fn hwpx2md_missing_input_exits_1() {
    let guard = TempDirGuard::new("missing-input");
    let input = guard.child("input.hwpx");
    let output = guard.child("out.md");

    let result = Command::new(env!("CARGO_BIN_EXE_hwpx2md"))
        .arg(&input)
        .arg(&output)
        .output()
        .expect("hwpx2md 실행 실패");
    assert_processing_failure(&result, &input, &output, "hwpx2md");
}

#[test]
fn hwpx2md_recoverable_warnings_with_valid_output_exit_0() {
    // 실제 파서 경고 경로: rowCnt="1"인데 hp:tc가 없는 손상된 표는 건너뛰고
    // 경고를 남긴다(parse.rs의 damaged table 복구). 같은 문서의 정상 본문은
    // 살아서 마크다운으로 나와야 하고, 종료 코드는 0이다(경고는 stdout JSON
    // Report에 그대로 노출).
    let guard = TempDirGuard::new("warnings-exit-0");
    let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:tbl rowCnt="1"><hp:tr></hp:tr></hp:tbl></hp:p><hp:p><hp:run><hp:t>본문 텍스트</hp:t></hp:run></hp:p></hp:hs>"#;
    let bytes = hwpx::shared::zip_read::test_fixtures::fixture_hwpx(&[(
        "Contents/section0.xml",
        section,
    )]);
    let input = guard.child("damaged.hwpx");
    std::fs::write(&input, &bytes).expect("픽스처 쓰기 실패");
    let output = guard.child("damaged.md");

    let result = Command::new(env!("CARGO_BIN_EXE_hwpx2md"))
        .arg(&input)
        .arg(&output)
        .output()
        .expect("hwpx2md 실행 실패");

    let code = result.status.code();
    assert_eq!(
        code,
        Some(0),
        "복구 가능한 경고가 있는 유효 변환은 종료 코드 0이어야 한다(실제 {code:?})"
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("damaged table skipped"),
        "stdout JSON Report에 손상 표 경고가 그대로 남아야 한다: {stdout:?}"
    );
    let markdown = std::fs::read_to_string(&output)
        .expect("경고가 있어도 유효한 출력 파일은 존재해야 한다");
    assert!(
        markdown.contains("본문 텍스트"),
        "손상 표와 무관한 본문은 살아 있어야 한다: {markdown:?}"
    );
}

// ---------------------------------------------------------------------------
// 규약 전체 커버리지(0 성공 / 1 처리·검증·I-O / 2 사용법)
// ---------------------------------------------------------------------------

fn md2hwpx_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_md2hwpx"))
}

fn hwpx2md_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hwpx2md"))
}

/// 저장소 안 커밋된 자산(README 수동 스모크와 같은 파일).
fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// 유효한 입력 픽스처를 임시 디렉터리에 쓴다(손상 표 + 본문).
fn write_damaged_table_fixture(guard: &TempDirGuard) -> PathBuf {
    let section = r#"<?xml version="1.0" encoding="utf-8"?><hp:hs xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph"><hp:p><hp:run><hp:t>본문 텍스트</hp:t></hp:run></hp:p></hp:hs>"#;
    let bytes =
        hwpx::shared::zip_read::test_fixtures::fixture_hwpx(&[("Contents/section0.xml", section)]);
    let input = guard.child("valid.hwpx");
    std::fs::write(&input, &bytes).expect("픽스처 쓰기 실패");
    input
}

#[test]
fn success_and_usage_codes_full_matrix() {
    // 0: 성공(양쪽 CLI). md2hwpx는 실제 example.md 컴파일,
    //    hwpx2md는 실제 변환으로 산출물을 검증한다.
    let guard = TempDirGuard::new("success");
    let out_hwpx = guard.child("compiled.hwpx");
    let result = md2hwpx_command()
        .arg(repo_file("web/public/example.md"))
        .arg(repo_file("web/public/template.hwpx"))
        .arg(&out_hwpx)
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(result.status.code(), Some(0), "md2hwpx 성공은 0");
    assert!(
        out_hwpx.is_file(),
        "성공 시 HWPX 출력 파일이 존재해야 한다"
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("\"validation\""),
        "stdout에 JSON Report가 나와야 한다: {stdout:?}"
    );

    let input = write_damaged_table_fixture(&guard);
    let out_md = guard.child("valid.md");
    let result = hwpx2md_command()
        .arg(&input)
        .arg(&out_md)
        .output()
        .expect("hwpx2md 실행 실패");
    assert_eq!(result.status.code(), Some(0), "hwpx2md 성공은 0");
    let markdown = std::fs::read_to_string(&out_md).expect("출력 파일이 있어야 한다");
    assert!(markdown.contains("본문 텍스트"), "{markdown:?}");

    // 0: help/version(양쪽). 플래그마다 새 프로세스를 구동한다.
    for bin in ["md2hwpx", "hwpx2md"] {
        for flag in ["-h", "--help", "-v", "--version"] {
            let mut cmd = if bin == "md2hwpx" {
                md2hwpx_command()
            } else {
                hwpx2md_command()
            };
            let result = cmd.arg(flag).output().expect("CLI 실행 실패");
            assert_eq!(
                result.status.code(),
                Some(0),
                "{bin} {flag}는 0이어야 한다"
            );
            assert!(
                !result.stdout.is_empty(),
                "{bin} {flag}는 사용법/버전을 stdout에 출력한다"
            );
        }
    }

    // 2: 모르는 옵션(양쪽), parse 필수 입력 누락, compile bad --variant.
    for (bin, mut cmd) in [("md2hwpx", md2hwpx_command()), ("hwpx2md", hwpx2md_command())] {
        let result = cmd.arg("--nope").output().expect("CLI 실행 실패");
        assert_eq!(result.status.code(), Some(2), "{bin} 모르는 옵션은 2");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("unrecognized option"),
            "{bin}: 사용법 오류 메시지가 stderr에 나와야 한다: {stderr:?}"
        );
    }
    let result = hwpx2md_command().output().expect("hwpx2md 실행 실패");
    assert_eq!(result.status.code(), Some(2), "입력 누락은 사용법 오류 2");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("missing required input file"),
        "{stderr:?}"
    );
    let result = md2hwpx_command()
        .arg("--variant")
        .arg("BROKEN")
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(
        result.status.code(),
        Some(2),
        "--variant CATEGORY=NAME 위반은 2"
    );
}

#[test]
fn corrupt_input_and_template_and_output_io_exits_1() {
    // 1: 처리/I-O 실패. 손상 바이트 입력과 쓰기 불가 출력 모두 실행 실패다.
    let guard = TempDirGuard::new("io-errors");

    // 손상된 템플릿(md2hwpx) — zip이 아닌 바이트.
    let bad_template = guard.child("bad-template.hwpx");
    std::fs::write(&bad_template, b"definitely not a zip archive").unwrap();
    let result = md2hwpx_command()
        .arg(repo_file("web/public/example.md"))
        .arg(&bad_template)
        .arg(guard.child("out1.hwpx"))
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(
        result.status.code(),
        Some(1),
        "손상된 템플릿은 처리 실패 1(실제 {:?})",
        result.status.code()
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("error:"),
        "{:?}",
        String::from_utf8_lossy(&result.stderr)
    );

    // 손상된 입력(hwpx2md).
    let bad_input = guard.child("bad-input.hwpx");
    std::fs::write(&bad_input, b"definitely not a zip archive").unwrap();
    let result = hwpx2md_command()
        .arg(&bad_input)
        .arg(guard.child("out2.md"))
        .output()
        .expect("hwpx2md 실행 실패");
    assert_eq!(
        result.status.code(),
        Some(1),
        "손상된 입력은 처리 실패 1(실제 {:?})",
        result.status.code()
    );

    // 출력 I/O 실패(양쪽) — 출력 경로가 디렉터리면 쓰기가 실패한다.
    let out_dir = guard.child("out-dir");
    std::fs::create_dir_all(&out_dir).unwrap();
    let result = md2hwpx_command()
        .arg(repo_file("web/public/example.md"))
        .arg(repo_file("web/public/template.hwpx"))
        .arg(&out_dir)
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(result.status.code(), Some(1), "출력 I/O 실패는 1");
    let input = write_damaged_table_fixture(&guard);
    let result = hwpx2md_command()
        .arg(&input)
        .arg(&out_dir)
        .output()
        .expect("hwpx2md 실행 실패");
    assert_eq!(result.status.code(), Some(1), "출력 I/O 실패는 1");
}

#[test]
fn md2hwpx_validation_errors_exit_1_and_warnings_stay_0() {
    // 1: 검증 errors — 컴파일은 성공해 출력이 나오지만, 소스에 남은 템플릿
    //    표지 토큰([[table]]은 소스 방언이 아니므로 평문으로 흘러간다)을
    //    검증기가 잡아 종료 코드 1이 된다. stdout JSON에는 errors가 실린다.
    let guard = TempDirGuard::new("validation");
    let source = guard.child("marker.md");
    std::fs::write(&source, "# 검토 보고서\n\n- 일반 항목\n[[table]]\n").unwrap();
    let output = guard.child("flagged.hwpx");
    let result = md2hwpx_command()
        .arg(&source)
        .arg(repo_file("web/public/template.hwpx"))
        .arg(&output)
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(
        result.status.code(),
        Some(1),
        "검증 errors는 1이어야 한다(실제 {:?})",
        result.status.code()
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("template marker remains"),
        "검증 errors가 stdout JSON에 나와야 한다: {stdout:?}"
    );
    assert!(output.is_file(), "컴파일 산출물은 존재한다");

    // 0: 프로토타입 샘플 텍스트 경고만 있는 경우 — 경고는 stdout JSON에
    //    남지만 종료 코드는 0이다.
    let source = guard.child("sample.md");
    std::fs::write(&source, "# 검토 보고서\n\n- 샘플 문장입니다\n").unwrap();
    let output = guard.child("warned.hwpx");
    let result = md2hwpx_command()
        .arg(&source)
        .arg(repo_file("web/public/template.hwpx"))
        .arg(&output)
        .output()
        .expect("md2hwpx 실행 실패");
    assert_eq!(
        result.status.code(),
        Some(0),
        "경고만 있는 성공은 0이어야 한다(실제 {:?})",
        result.status.code()
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("prototype sample text remains"),
        "샘플 텍스트 경고가 stdout JSON에 나와야 한다: {stdout:?}"
    );
}
