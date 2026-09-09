import init, { compile_hwpx, convert_hwpx } from './wasm/hwpx_wasm.js'

let wasmInit: Promise<void> | null = null

/** 공유 wasm 엔진을 초기화한다(메모이즈드, 실패 시 리셋). */
export function ensureWasm(): Promise<void> {
  if (!wasmInit) {
    wasmInit = init().then(() => undefined)
    wasmInit.catch(() => {
      wasmInit = null
    })
  }
  return wasmInit
}

/** 마크다운 + template.hwpx 바이트 → HWPX 바이트 (MD2HWPX용). */
export { compile_hwpx }

/** HWPX 바이트 → `{"markdown","warnings"}` JSON 문자열 (HWPX2MD용). */
export { convert_hwpx }
