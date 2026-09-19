/** 파일명으로 쓸 수 없는 문자 및 보안 위험 문자(Zero-width, Bidi, 경로 순회) 제거. 정리 결과 빈 문자열이면 fallback을 쓴다. */
export function sanitizeFilename(name: string, fallback: string): string {
  const cleaned = name
    // 짝이 맞지 않는 단독 surrogate 제거 (짝이 맞는 surrogate pair만 보존)
    .replace(/([\uD800-\uDBFF][\uDC00-\uDFFF])|[\uD800-\uDFFF]/g, '$1')
    // Zero-width 및 비가시 유니코드 제거
    .replace(/[\u200B-\u200D\uFEFF]/g, '')
    // 유니코드 Bidi 제어 문자 제거 (LRO, RLO, LRE, RLE, PDF 등)
    .replace(/[\u202A-\u202E\u2066-\u2069]/g, '')
    // 상위 경로 순회 패턴 제거
    .replace(/\.\.+[/\\]/g, '')
    // 파일명 금지 문자 및 ASCII 제어 문자 제거
    .replace(/[\\/:*?"<>|\x00-\x1f]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/[. ]+$/, '')
  return cleaned || fallback
}
