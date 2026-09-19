use std::fmt;
use std::path::Path;

/// 모든 컴파일 오류를 문자열로 담는 단순 오류 형식.
/// hwp.py가 ValueError/OSError/BadZipFile/ParseError를 한 줄 메시지로
/// 출력하는 것과 같은 자리를 대체한다.
#[derive(Debug)]
pub struct AppError(pub String);

/// 파일 접근 실패 메시지의 공통 형식: 파일·동작·OS 오류를 한 줄에 모은다.
pub fn io_context(action: String, e: std::io::Error) -> AppError {
    AppError(format!("{action}: {e}"))
}

/// 파일 열기 실패 시 어느 파일인지 반드시 밝힌다. OS 메시지만 출력하면
/// source와 template 중 무엇이 없는지 알 수 없다.
pub fn open_error(path: &Path, e: std::io::Error) -> AppError {
    io_context(format!("cannot open {}", path.display()), e)
}

/// 쓰기용 파일 생성 실패는 읽기 열기와 구별해서 밝힌다(랜섬웨어 백신이
/// 가장 먼저 막는 지점).
pub fn create_error(path: &Path, e: std::io::Error) -> AppError {
    io_context(format!("cannot create {} for writing", path.display()), e)
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError(e.to_string())
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(e: zip::result::ZipError) -> Self {
        AppError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

pub fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(AppError(msg.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_error_names_file() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        let msg = open_error(Path::new("workspace/template.hwpx"), e).0;
        assert!(
            msg.starts_with("cannot open workspace/template.hwpx: "),
            "{msg}"
        );
    }

    #[test]
    fn create_error_distinguishes_write_from_open() {
        let e = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let msg = create_error(Path::new("compiled.hwpx"), e).0;
        assert!(
            msg.starts_with("cannot create compiled.hwpx for writing: "),
            "{msg}"
        );
    }
}
