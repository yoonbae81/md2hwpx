/** 파일명으로 쓸 수 없는 문자 제거. 정리 결과 빈 문자열이면 fallback을 쓴다. */
export function sanitizeFilename(name: string, fallback: string): string {
  const cleaned = name
    .replace(/[\\/:*?"<>|\x00-\x1f]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/[. ]+$/, '')
  return cleaned || fallback
}
