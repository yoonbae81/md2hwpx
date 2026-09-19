/** 인쇄 미리보기용 마크다운 → HTML 근사 렌더러(M2hPage·H2mPage 공용).
 *
 * src/compile(source.rs·patterns.rs)와 src/shared(glyph.rs·dialect.rs)의
 * 소스 방언 해석 규칙을 미러링하지만, 출력은 HWPX가 아니라 A4 인쇄 근사
 * HTML이다. 모양은 template.hwpx 프로토타입 대신 소비자 print CSS가
 * `pv-*` 클래스로 입힌다(`pv-avoid-break` = 페이지 나눔 회피 대상).
 * 순수 함수로 DOM 부수효과가 없고, 원문은 항상 HTML 이스케이프 뒤 인라인
 * `**굵게**`만 적용해 삽입한다(XSS 안전).
 *
 * 정규식은 lookahead·lookbehind·역참조 없이 Wasm 엔진의 regex-lite
 * 서브셋(리터럴 한글, \d/\s, 기본 반복)과 호환되는 범위를 유지한다.
 */
export function renderPreviewMarkdown(md: string): string {
  const text = md.replace(/^\ufeff/, '').replace(/\r\n/g, '\n')
  const lines = text.split('\n').map(stripQuoteMarkers)
  const blocks = parseBlocks(lines)
  resolveBulletDepths(blocks)
  return renderBlocks(blocks)
}

// ── 방언 토큰/정규식(src/compile/patterns.rs·source.rs와 대응) ──

/** 가로선이자 하이라이트 경계. 5줄 창 규칙으로만 구분한다(dialect.rs). */
const HRULE = '---'
/** 펜스 비교용 트림. JS trim()이 못 걷는 제로폭 공백까지 제거한다
 * (BOM은 trim이 처리). source.rs fence_trim과 같은 판정이어야 한다. */
function fenceTrim(s: string): string {
  return s.replace(/^[\s\ufeff\u200b]+|[\s\ufeff\u200b]+$/g, '')
}
/** 닫는 `---`가 열린 `---`로부터 이 줄 수 이내일 때만 하이라이트 경계. */
const HIGHLIGHT_WINDOW = 5

const FRONT_KEY_RE = /^[A-Za-z_][\w-]*\s*:/
const TABLE_SEP_CELL_RE = /^:?-{3,}:?$/
const BOX_RE = /^(?:\*\*)?\[{1,2}(?:박스|box)\]{1,2}\s*(.*?)(?:\*\*)?$/i
const TITLE_COMMAND_RE = /^\[\[(붙임|외부제목)\]\]\s*(.*)$/
const MD_HEADING_RE = /^(#{1,6})\s+(.+?)\s*#*\s*$/
const NUM_SUB_RE = /^\d+\.\d+\b/
const NUM_RE = /^(\d+\.)\s*(.*)$/
const BRACKET_TITLE_RE = /^\[(?:제목|title)\]/i
const BRACKET_TITLE_STRIP_RE = /^\[(?:제목|title)\]\s*[:：]?\s*/i
const GIYEOK_TITLE_RE = /^제\s*목\s*[:：]?\s*/
const NOTE_MARKER_RE = /^\\?([*※])\s+(.*)$/
const MD_CORE_RE = /^(?:\*\*|__)(.*)(?:\*\*|__)$/
const HL_BULLET_RE = /^[-*※]\s*/
const BOLD_RE = /\*\*([^*]+)\*\*/g
const QUOTE_MARKER_RE = /^([ \t]*)((?:> ?)+)(.*)$/

// ── 글리프 규칙(src/shared/glyph.rs와 대응) ──

const BULLET_GLYPHS = new Set([
  '-', '*', '+', '○', 'ㅇ', '◯', '●', '◦', '•', '·', '∙', '∘', '▪', '▫', '‣', '⁃',
  '□', '■', '◆', '◇', '▶', '▷', '⊙', '☞',
])
/** 원문자 번호(①~⑳)와 주석 글리프(☞)는 본문 앞에 남는다. */
const isCircledDigit = (c: string): boolean => c >= '①' && c <= '⑳'
const staysInBody = (c: string): boolean => isCircledDigit(c) || c === '☞'
const isBulletGlyph = (c: string): boolean => BULLET_GLYPHS.has(c) || isCircledDigit(c)
const isVisibleSubItem = (c: string): boolean => c === '○' || c === 'ㅇ' || staysInBody(c)

// ── 블록 모델 ──

type BulletBlock = { kind: 'bullet'; indent: number; glyph: string; text: string; depth: number }
type Block =
  | { kind: 'title'; text: string }
  | { kind: 'heading'; text: string }
  | { kind: 'subheading'; text: string }
  | { kind: 'para'; lines: string[] }
  | BulletBlock
  | { kind: 'reference'; marker: string; text: string }
  | { kind: 'box'; title: string; items: string[] }
  | { kind: 'highlight'; items: string[] }
  | { kind: 'table'; rows: string[][] }
  | { kind: 'banner'; variant: 'attach' | 'external'; text: string }
  | { kind: 'hr' }

// ── 문자열 헬퍼 ──

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/** 이스케이프 먼저, 그 위에 인라인 `**굵게**`만 얹는다. */
function inline(s: string): string {
  return escapeHtml(s).replace(BOLD_RE, '<strong>$1</strong>')
}

/** 이메일 인용(`>`, `>>`, `> >`) 표기를 줄 앞에서 걷어낸다. 들여쓰기 보존. */
function stripQuoteMarkers(line: string): string {
  const m = QUOTE_MARKER_RE.exec(line)
  return m ? m[1] + m[3] : line
}

/** 선행 공백/탭의 들여쓰기 폭. 탭은 4칸 탭 스톱. */
function indentWidth(line: string): number {
  let col = 0
  for (const ch of line) {
    if (ch === ' ') col += 1
    else if (ch === '\t') col = Math.floor(col / 4) * 4 + 4
    else break
  }
  return col
}

/** 줄 앞의 불릿 글리프와 나머지. '*'는 뒤에 공백이 올 때만 불릿(`**` 구별). */
function splitBullet(stripped: string): [string, string] | null {
  const glyph = [...stripped][0]
  if (glyph === undefined || !isBulletGlyph(glyph)) return null
  const rest = stripped.slice(glyph.length)
  if (glyph === '*' && !/^\s/.test(rest)) return null
  return [glyph, rest]
}

/** 글리프 뒤 본문. 본문에 남는 글리프(원문자 번호·☞)는 앞에 되살린다. */
function bulletItemBody(glyph: string, rest: string): string {
  const body = glyph === '*' ? rest.replace(/^\s+/, '') : rest.trim()
  if (staysInBody(glyph)) return body ? `${glyph} ${body}` : glyph
  return body
}

/** 본문 첫 글자가 남는 글리프면 그 글리프가 항목의 성격을 결정한다. */
function resolveItemGlyph(glyph: string, body: string): string {
  const first = [...body][0]
  return first !== undefined && staysInBody(first) ? first : glyph
}

/** `**굵게**`/`__굵게__` 한 겹을 벗겨낸다(번호 제목 판별용). */
function markdownCore(text: string): string {
  const m = MD_CORE_RE.exec(text.trim())
  return m ? m[1] : text.trim()
}

function tableRow(text: string): string[] {
  return text
    .trim()
    .replace(/^\|+/, '')
    .replace(/\|+$/, '')
    .split('|')
    .map((c) => c.trim())
}

/** 제목 후보에서 제외되는 목록성 줄인가. */
function isListItem(stripped: string): boolean {
  return stripped.startsWith('※') || stripped.startsWith('*') || splitBullet(stripped) !== null
}

// ── 1단계: 줄 → 블록(source.rs의 SourceParser 미러) ──

/** front matter 경계와 하이라이트 `---` 쌍(5줄 창 규칙)을 미리 가린다. */
function prescan(lines: string[]): { firstIndex: number; inFrontMatter: boolean; highlightDelims: Set<number> } {
  let firstIndex = -1
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim()) {
      firstIndex = i
      break
    }
  }
  let secondIndex = -1
  for (let i = firstIndex + 1; i < lines.length; i++) {
    if (fenceTrim(lines[i]) === HRULE) {
      secondIndex = i
      break
    }
  }
  const metadataIsFront =
    firstIndex >= 0 &&
    secondIndex >= 0 &&
    lines.slice(firstIndex + 1, secondIndex).some((l) => FRONT_KEY_RE.test(l.trim()))
  const inFrontMatter = firstIndex >= 0 && fenceTrim(lines[firstIndex]) === HRULE && metadataIsFront
  const delimiterStart = inFrontMatter ? secondIndex + 1 : 0
  // 문서 첫 줄의 `---`는 가로선/front matter 전용이라 하이라이트 경계에서 뺀다.
  const docFirstDash = firstIndex >= 0 && fenceTrim(lines[firstIndex]) === HRULE ? firstIndex : -1
  const delimiters: number[] = []
  for (let i = delimiterStart; i < lines.length; i++) {
    if (fenceTrim(lines[i]) === HRULE && i !== docFirstDash) delimiters.push(i)
  }
  const highlightDelims = new Set<number>()
  for (let pair = 0; pair + 1 < delimiters.length; ) {
    const open = delimiters[pair]
    const close = delimiters[pair + 1]
    const withinWindow = close - open <= HIGHLIGHT_WINDOW
    const hasContent = lines.slice(open + 1, close).some((l) => l.trim().length > 0)
    if (withinWindow && hasContent) {
      highlightDelims.add(open)
      highlightDelims.add(close)
      pair += 2
    } else {
      pair += 1
    }
  }
  return { firstIndex, inFrontMatter, highlightDelims }
}

function parseBlocks(lines: string[]): Block[] {
  const { firstIndex, inFrontMatter: startsInFront, highlightDelims } = prescan(lines)
  const blocks: Block[] = []
  let titleSet = false
  let fallbackTitle = ''
  let inFrontMatter = startsInFront
  let boxOpen: { title: string; items: string[]; indent: number } | null = null
  let inHighlight = false
  let highlightLines: string[] = []

  const closeBox = () => {
    if (boxOpen) {
      blocks.push({ kind: 'box', title: boxOpen.title, items: boxOpen.items })
      boxOpen = null
    }
  }
  const pushPara = (text: string) => {
    const last = blocks.at(-1)
    if (last !== undefined && last.kind === 'para') last.lines.push(text)
    else blocks.push({ kind: 'para', lines: [text] })
  }

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].replace(/\s+$/, '')
    const stripped = line.trim()
    if (!stripped) continue

    // 열린 박스 아래 더 깊이 들여쓰인 불릿 줄은 박스 항목으로 모은다.
    if (boxOpen) {
      const item = fenceTrim(stripped) !== HRULE ? splitBullet(stripped) : null
      if (item && indentWidth(line) > boxOpen.indent) {
        boxOpen.items.push(bulletItemBody(item[0], item[1]))
        continue
      }
      closeBox()
    }

    if (fenceTrim(stripped) === HRULE) {
      if (inFrontMatter) {
        if (i !== firstIndex) inFrontMatter = false
        continue
      }
      if (highlightDelims.has(i)) {
        if (inHighlight) {
          const items = highlightLines
            .map((l) => l.trim().replace(HL_BULLET_RE, ''))
            .filter((l) => l.length > 0)
          if (items.length > 0) blocks.push({ kind: 'highlight', items })
          highlightLines = []
          inHighlight = false
        } else {
          inHighlight = true
        }
      } else {
        // 짝을 이루지 못한 `---`는 가로선. compile은 무시하지만 미리보기는
        // 시각적 구분으로 hr를 그린다.
        blocks.push({ kind: 'hr' })
      }
      continue
    }
    if (inFrontMatter) continue

    const command = TITLE_COMMAND_RE.exec(stripped)
    if (command) {
      blocks.push({
        kind: 'banner',
        variant: command[1] === '붙임' ? 'attach' : 'external',
        text: command[2].trim(),
      })
      continue
    }
    if (inHighlight) {
      highlightLines.push(stripped)
      continue
    }

    // 마크다운 표: 헤더 + 구분줄 쌍이면 이후 `|` 행을 모두 소비한다.
    if (stripped.includes('|') && i + 1 < lines.length) {
      const header = tableRow(stripped)
      const separator = tableRow(lines[i + 1].trim())
      if (
        header.length > 1 &&
        separator.length === header.length &&
        separator.every((c) => TABLE_SEP_CELL_RE.test(c))
      ) {
        const rows = [header]
        let end = i + 2
        while (end < lines.length) {
          const t = lines[end].trim()
          if (!t || !t.includes('|')) break
          rows.push(tableRow(t))
          end++
        }
        blocks.push({ kind: 'table', rows })
        i = end - 1
        continue
      }
    }

    const box = BOX_RE.exec(stripped)
    if (box) {
      boxOpen = { title: box[1].trim(), items: [], indent: indentWidth(line) }
      continue
    }

    const heading = MD_HEADING_RE.exec(stripped)
    if (heading) {
      const level = heading[1].length
      const text = heading[2].trim()
      if (level > 1) {
        blocks.push(
          level >= 3 || NUM_SUB_RE.test(markdownCore(text))
            ? { kind: 'subheading', text }
            : { kind: 'heading', text },
        )
        continue
      }
      if (!titleSet) {
        titleSet = true
        blocks.push({ kind: 'title', text })
        continue
      }
      // 제목 확정 뒤의 `#` 줄은 compile처럼 일반 본문 경로로 흘러간다.
    }

    if (!titleSet) {
      if (BRACKET_TITLE_RE.test(stripped)) {
        titleSet = true
        blocks.push({ kind: 'title', text: stripped.replace(BRACKET_TITLE_STRIP_RE, '') })
        continue
      }
      if (stripped.startsWith('제목') || stripped.startsWith('제 목')) {
        titleSet = true
        blocks.push({ kind: 'title', text: stripped.replace(GIYEOK_TITLE_RE, '') })
        continue
      }
    }

    const core = markdownCore(stripped)
    if (NUM_SUB_RE.test(core)) {
      blocks.push({ kind: 'subheading', text: stripped })
      continue
    }
    const num = NUM_RE.exec(core)
    if (num) {
      if (!fallbackTitle) fallbackTitle = num[2].trim()
      blocks.push({ kind: 'heading', text: stripped })
      continue
    }

    if (!titleSet && !isListItem(stripped)) {
      titleSet = true
      blocks.push({ kind: 'title', text: stripped })
      continue
    }

    // 본문 줄: 각주(※) · 보조표기(들여쓴 *) · 불릿 · 문단.
    const indent = indentWidth(line)
    if (stripped.startsWith('※')) {
      blocks.push({ kind: 'reference', marker: '※', text: stripped.slice(1).replace(/^\s+/, '') })
      continue
    }
    const bullet = splitBullet(stripped)
    if (!bullet) {
      pushPara(stripped)
      continue
    }
    const [glyph, rest] = bullet
    if (glyph === '*' && indent > 0) {
      blocks.push({ kind: 'reference', marker: '*', text: rest.replace(/^\s+/, '') })
      continue
    }
    if (glyph === '-') {
      const note = NOTE_MARKER_RE.exec(rest.trim())
      if (note) {
        blocks.push({ kind: 'reference', marker: note[1], text: note[2].trim() })
        continue
      }
    }
    const body = bulletItemBody(glyph, rest)
    blocks.push({ kind: 'bullet', indent, glyph: resolveItemGlyph(glyph, body), text: body, depth: 1 })
  }

  closeBox()
  if (inHighlight && highlightLines.length > 0) {
    // 닫는 `---` 없이 끝난 강조 블록: compile은 오류지만 미리보기는 모은
    // 내용이라도 그린다.
    const items = highlightLines
      .map((l) => l.trim().replace(HL_BULLET_RE, ''))
      .filter((l) => l.length > 0)
    if (items.length > 0) blocks.push({ kind: 'highlight', items })
  }
  // 제목 줄이 없으면 compile처럼 첫 번째 `N.` 제목의 본문을 표지 제목으로 쓴다.
  if (!titleSet && fallbackTitle) blocks.unshift({ kind: 'title', text: fallbackTitle })
  return blocks
}

// ── 2단계: 불릿 깊이 확정(source.rs finish_bullets 미러) ──

function resolveBulletDepths(blocks: Block[]): void {
  const bullets = blocks.filter((b): b is BulletBlock => b.kind === 'bullet')
  const ranks = [...new Set(bullets.map((b) => b.indent))].sort((a, z) => a - z)
  let prev: { indent: number; depth: number; glyph: string } | null = null
  for (const b of bullets) {
    let depth = Math.min(ranks.indexOf(b.indent) + 1, 3)
    // "보이는 하위 항목" 글리프(○/ㅇ·원문자 번호·☞)는 같은 들여쓰기의 다른
    // 기호 바로 아래에서 한 단계 더 깊게 본다.
    if (prev !== null && b.indent === prev.indent && isVisibleSubItem(b.glyph)) {
      depth = isVisibleSubItem(prev.glyph) ? prev.depth : Math.min(depth + 1, 3)
    }
    b.depth = depth
    prev = { indent: b.indent, depth, glyph: b.glyph }
  }
}

// ── 3단계: 블록 → HTML ──

function renderBlocks(blocks: Block[]): string {
  const out: string[] = []
  let listOpen = false
  const flushList = () => {
    if (listOpen) {
      out.push('</ul>')
      listOpen = false
    }
  }
  for (const b of blocks) {
    if (b.kind !== 'bullet') flushList()
    switch (b.kind) {
      case 'title':
        out.push(`<h1 class="pv-title">${inline(b.text)}</h1>`)
        break
      case 'heading':
        out.push(`<h2 class="pv-h2">${inline(b.text)}</h2>`)
        break
      case 'subheading':
        out.push(`<h3 class="pv-h3">${inline(b.text)}</h3>`)
        break
      case 'para':
        out.push(`<p class="pv-para">${b.lines.map(inline).join('<br>')}</p>`)
        break
      case 'bullet':
        if (!listOpen) {
          out.push('<ul class="pv-list">')
          listOpen = true
        }
        out.push(`<li class="pv-item pv-depth${b.depth}">${inline(b.text)}</li>`)
        break
      case 'reference':
        out.push(`<p class="pv-reference">${b.marker} ${inline(b.text)}</p>`)
        break
      case 'box': {
        const title = b.title ? `<p class="pv-box-title">${inline(b.title)}</p>` : ''
        const items = b.items.length
          ? `<ul class="pv-box-items">${b.items
              .map((it) => `<li class="pv-box-item">${inline(it)}</li>`)
              .join('')}</ul>`
          : ''
        out.push(`<section class="pv-box pv-avoid-break">${title}${items}</section>`)
        break
      }
      case 'highlight':
        out.push(
          `<section class="pv-highlight pv-avoid-break">${b.items
            .map((it) => `<p class="pv-highlight-item">${inline(it)}</p>`)
            .join('')}</section>`,
        )
        break
      case 'table': {
        const [head, ...body] = b.rows
        const thead = `<thead class="pv-thead"><tr class="pv-tr">${head
          .map((c) => `<th class="pv-th">${inline(c)}</th>`)
          .join('')}</tr></thead>`
        const tbody = body.length
          ? `<tbody class="pv-tbody">${body
              .map(
                (r) =>
                  `<tr class="pv-tr">${r.map((c) => `<td class="pv-td">${inline(c)}</td>`).join('')}</tr>`,
              )
              .join('')}</tbody>`
          : ''
        out.push(`<table class="pv-table pv-avoid-break">${thead}${tbody}</table>`)
        break
      }
      case 'banner': {
        const label = b.variant === 'attach' ? '붙임' : '외부제목'
        const title = b.text ? `<span class="pv-banner-title">${inline(b.text)}</span>` : ''
        out.push(
          `<section class="pv-banner ${b.variant === 'attach' ? 'pv-attach' : 'pv-external'}">` +
            `<span class="pv-banner-label">${label}</span>${title}</section>`,
        )
        break
      }
      case 'hr':
        out.push('<hr class="pv-hr">')
        break
    }
  }
  flushList()
  return out.join('\n')
}
