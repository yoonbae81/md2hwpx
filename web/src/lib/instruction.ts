/** instruction.md용 제한된 마크다운 렌더러.
 * 지원: # · ## · ### 제목, 번호/불릿 목록, 문단(줄바꿈 보존), **강조**.
 * 나머지 문법은 일반 텍스트 문단으로 흘러간다. */
export function renderInstructionMarkdown(md: string): string {
  const escape = (s: string) =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const inline = (s: string) =>
    escape(s).replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')

  const out: string[] = []
  let para: string[] = []
  let list: { tag: 'ol' | 'ul'; items: string[] } | null = null

  const flushPara = () => {
    if (para.length) {
      out.push(`<p>${para.map(inline).join('<br>')}</p>`)
      para = []
    }
  }
  const flushList = () => {
    if (list) {
      out.push(
        `<${list.tag}>${list.items.map((i) => `<li>${inline(i)}</li>`).join('')}</${list.tag}>`,
      )
      list = null
    }
  }

  for (const raw of md.replace(/\r\n/g, '\n').split('\n')) {
    const line = raw.trimEnd()
    if (!line.trim()) {
      flushPara()
      flushList()
      continue
    }
    const heading = /^(#{1,3})\s+(.*)$/.exec(line)
    if (heading) {
      flushPara()
      flushList()
      const level = heading[1].length
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }
    const ordered = /^\d+\.\s+(.*)$/.exec(line)
    if (ordered) {
      flushPara()
      if (list?.tag !== 'ol') {
        flushList()
        list = { tag: 'ol', items: [] }
      }
      list.items.push(ordered[1])
      continue
    }
    const bulleted = /^-\s+(.*)$/.exec(line)
    if (bulleted) {
      flushPara()
      if (list?.tag !== 'ul') {
        flushList()
        list = { tag: 'ul', items: [] }
      }
      list.items.push(bulleted[1])
      continue
    }
    flushList()
    para.push(line)
  }
  flushPara()
  flushList()
  return out.join('\n')
}
