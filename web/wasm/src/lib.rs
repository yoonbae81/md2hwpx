//! 웹앱(web)이 사용하는 단일 wasm 바인딩. MD2HWPX 및 HWPX2MD 두 기능의
//! 바인딩을 함께 제공하며, 함수 시그니처와 반환 JSON 형태는 동일하다.

use serde::Serialize;
use wasm_bindgen::prelude::*;

/// 마크다운 + template.hwpx 바이트를 HWPX 바이트로 변환한다.
/// 변형 선택은 웹에서는 사용하지 않는다(빈 목록).
#[wasm_bindgen]
pub fn compile_hwpx(source_md: &str, template_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    let (bytes, _) = hwpx::compile::compile_hwpx_bytes(source_md, template_bytes, &[])
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(bytes)
}

/// 변환 성공 시 웹으로 넘길 결과. 마크다운 본문과 `Report.warnings`(손상 표
/// 스킵 등 비치명 경고)만 프런트엔드가 쓴다. 구조화 값을 추가 바인딩 의존성
/// (serde-wasm-bindgen 등) 없이 넘기기 위해 JSON 문자열로 직렬화한다.
#[derive(Serialize)]
struct Converted {
    markdown: String,
    warnings: Vec<String>,
}

/// HWPX 바이트를 마크다운으로 변환한다. 성공 시
/// `{"markdown": string, "warnings": string[]}` JSON 문자열을 반환하고,
/// 실패 시 오류 메시지(AppError의 한 줄 영문)를 JsValue로 던진다.
#[wasm_bindgen]
pub fn convert_hwpx(input_bytes: &[u8], label: &str) -> Result<String, JsValue> {
    let (markdown, report) = hwpx::parse::hwpx_to_markdown_bytes(input_bytes, label)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_json::to_string(&Converted {
        markdown,
        warnings: report.warnings,
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}
