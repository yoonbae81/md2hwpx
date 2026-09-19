/* tslint:disable */
/* eslint-disable */

/**
 * 마크다운 + template.hwpx 바이트를 HWPX 바이트로 변환한다.
 * 변형 선택은 웹에서는 사용하지 않는다(빈 목록).
 */
export function compile_hwpx(source_md: string, template_bytes: Uint8Array): Uint8Array;

/**
 * HWPX 바이트를 마크다운으로 변환한다. 성공 시
 * `{"markdown": string, "warnings": string[]}` JSON 문자열을 반환하고,
 * 실패 시 오류 메시지(AppError의 한 줄 영문)를 JsValue로 던진다.
 */
export function convert_hwpx(input_bytes: Uint8Array, label: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly compile_hwpx: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly convert_hwpx: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
