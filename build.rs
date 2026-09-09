fn main() {
    let now = chrono::Local::now();
    println!("cargo:rustc-env=BUILD_DATE={}", now.format("%Y-%m-%d"));
    embed_version_info(&now.format("%Y-%m-%d").to_string());
}

/// exe에 버전 리소스(제품명·설명·버전)를 심는다. 백신 휴리스틱은 이
/// 메타데이터가 비어 있는 서명 없는 exe에 감점하므로 Cargo 메타데이터로
/// 채워 둔다.
#[cfg(windows)]
fn embed_version_info(build_date: &str) {
    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "md2hwpx");
    res.set("FileDescription", env!("CARGO_PKG_DESCRIPTION"));
    res.set("FileVersion", env!("CARGO_PKG_VERSION"));
    res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    res.set("OriginalFilename", "md2hwpx.exe");
    res.set("Comments", &format!("빌드 {build_date}"));
    res.compile().expect("버전 리소스 생성 실패");
}

#[cfg(not(windows))]
fn embed_version_info(_build_date: &str) {}
