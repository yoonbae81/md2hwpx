/** 변환/템플릿 입력 파일 크기 상한(바이트). 파일을 읽기(arrayBuffer) 전에 검사한다. */
export const MAX_INPUT_BYTES = 100 * 1024 * 1024

/** 크기 초과 파일을 거절할 때 쓰는 사용자 안내 문구. */
export function inputTooLargeMessage(): string {
  return `파일이 너무 큽니다 (최대 ${MAX_INPUT_BYTES / (1024 * 1024)}MB)`
}
