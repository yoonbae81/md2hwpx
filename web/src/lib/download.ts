import { sanitizeFilename } from './sanitize'

const FALLBACK_M2H_TITLE = '문서'
const FALLBACK_H2M_STEM = 'converted'

/** 마크다운 최상단 `# 제목` 추출 (없으면 '문서') */
export function extractTitle(markdown: string): string {
  const m = /^#\s+(.+)$/m.exec(markdown)
  return m ? m[1].trim() : FALLBACK_M2H_TITLE
}

/** `yymmdd 제목 hhmmss.hwpx` 형식 파일명 생성 (MD → HWPX) */
export function buildM2hFilename(markdown: string): string {
  const now = new Date()
  const p2 = (n: number) => String(n).padStart(2, '0')
  const date = `${String(now.getFullYear()).slice(2)}${p2(now.getMonth() + 1)}${p2(now.getDate())}`
  const time = `${p2(now.getHours())}${p2(now.getMinutes())}${p2(now.getSeconds())}`
  return `${date} ${sanitizeFilename(extractTitle(markdown), FALLBACK_M2H_TITLE)} ${time}.hwpx`
}

/** HWPX 바이트 다운로드 */
export function downloadBytes(bytes: Uint8Array, filename: string): void {
  const blob = new Blob([bytes], { type: 'application/octet-stream' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/** CLI 규약(<입력줄기>.md)과 같은 출력 파일명: `원본파일명.hwpx` → `원본파일명.md` (HWPX → MD) */
export function buildH2mFilename(hwpxName: string): string {
  const stem = hwpxName.replace(/\.hwpx$/i, '')
  return `${sanitizeFilename(stem, FALLBACK_H2M_STEM)}.md`
}

/** 마크다운 텍스트 다운로드 */
export function downloadText(text: string, filename: string): void {
  const blob = new Blob([text], { type: 'text/markdown' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
