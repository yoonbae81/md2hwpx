<script lang="ts">
  import { onMount } from 'svelte'
  import {
    Check,
    Copy,
    Eye,
    EyeOff,
    LoaderCircle,
    Moon,
    Printer,
    Sun,
    SunMoon,
    TriangleAlert,
    Upload,
    X,
    Zap,
  } from 'lucide-svelte'
  import SwapHeader from '../components/SwapHeader.svelte'
  import { ensureWasm, convert_hwpx } from '../lib/wasm'
  import { cycleTheme, getTheme } from '../lib/theme.svelte'
  import { buildH2mFilename as buildFilename, downloadText } from '../lib/download'
  import { copyText } from '../lib/clipboard'
  import { renderPreviewMarkdown } from '../lib/preview'
  import { inputTooLargeMessage, MAX_INPUT_BYTES } from '../lib/limits'
  import '../lib/print.css'
  // [미러 포인트] cross-page 드롭 전달 — M2hPage도 같은 모듈을 쓴다.
  import { setPendingTransfer, takePendingTransfer } from '../lib/drop-transfer'

  let {
    onNavigate,
    active = true,
  }: {
    onNavigate?: (page: 'm2h' | 'h2m') => void
    active?: boolean
  } = $props()

  let text = $state('')

  // 편집/미리보기 토글. 변환 결과(text)는 양방향 전환에서 그대로 유지된다.
  let previewMode = $state(false)

  // 마지막으로 변환 성공한 원본 파일명(다운로드 파일명 프리뷰에 쓴다).
  let sourceName = $state('')
  let warnings = $state<string[]>([])

  let converting = $state(false)
  let copied = $state(false)
  let errorMessage = $state('')
  let textDragOver = $state(false)
  let boxDragOver = $state(false)

  let fileInput = $state<HTMLInputElement>()

  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let errorTimer: ReturnType<typeof setTimeout> | undefined

  const canDownload = $derived(text.trim().length > 0)
  const canPrint = $derived(text.trim().length > 0 && previewMode)
  const downloadName = $derived(sourceName !== '' ? buildFilename(sourceName) : 'converted.md')
  // 미리보기 HTML은 미리보기 모드에서만 계산한다(#print-area는 이 모드에서만 렌더링).
  const previewHtml = $derived(previewMode ? renderPreviewMarkdown(text) : '')

  onMount(() => {
    // 창 밖 드롭으로 브라우저가 파일을 열어버리는 현상 방지
    const prevent = (e: Event) => e.preventDefault()
    window.addEventListener('dragover', prevent)
    window.addEventListener('drop', prevent)

    // 가상 키보드 올라올 때 레이아웃 높이를 visualViewport에 맞춤
    const vv = window.visualViewport
    const syncVh = () => {
      if (vv) document.documentElement.style.setProperty('--vvh', `${vv.height}px`)
    }
    vv?.addEventListener('resize', syncVh)
    syncVh()

    return () => {
      window.removeEventListener('dragover', prevent)
      window.removeEventListener('drop', prevent)
      vv?.removeEventListener('resize', syncVh)
    }
  })

  function onThemeClick() {
    cycleTheme()
  }

  async function onCopyButton() {
    if (!text.trim()) return
    if (await copyText(text)) {
      copied = true
      clearTimeout(copyTimer)
      copyTimer = setTimeout(() => (copied = false), 1500)
    }
  }

  /** 짧은 인라인 오류 배너. 원시 영문(Rust AppError)은 노출하지 않는다. */
  function showErrorMessage(message: string) {
    errorMessage = message
    clearTimeout(errorTimer)
    errorTimer = setTimeout(() => (errorMessage = ''), 6000)
  }

  /** WASM이 던지는 한 줄 영문 오류를 한국어 안내로 매핑. */
  function toUserError(e: unknown): string {
    const raw = typeof e === 'string' ? e : e instanceof Error ? e.message : ''
    if (raw.includes('is too large')) return '파일이 너무 큽니다'
    if (
      raw.includes('Zip archive') ||
      raw.includes('DTD/entity declaration') ||
      raw.includes('no HWPX section files found')
    ) {
      return 'HWPX 파일이 아니거나 손상된 파일입니다'
    }
    return '변환에 실패했습니다'
  }

  function onDownload() {
    if (!canDownload || converting) return
    downloadText(text, downloadName)
  }

  /** 미리보기 인쇄: print.css 계약대로 body 클래스를 올리고 인쇄하며,
   *  afterprint에서 클래스를 치운다(리스너는 once로 쌓이지 않게 한다). */
  function onPrint() {
    if (!canPrint) return
    document.body.classList.add('printing-preview')
    window.addEventListener(
      'afterprint',
      () => document.body.classList.remove('printing-preview'),
      { once: true },
    )
    window.print()
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') previewMode = false
  }

  /** 단일 .md/.txt는 compile 페이지가 처리하므로 넘긴다. 넘기면 true. */
  function routeForeignFile(file: File): boolean {
    const name = file.name.toLowerCase()
    if (!name.endsWith('.md') && !name.endsWith('.txt')) return false
    setPendingTransfer(file, 'm2h')
    onNavigate?.('m2h')
    return true
  }

  // compile 페이지에서 넘어온 .hwpx 투입 (활성 전환 시 1회, 확인 없음).
  $effect(() => {
    if (!active) return
    const file = takePendingTransfer('h2m')
    if (file) void convertFile(file)
  })

  /** 파일 하나를 변환해 편집기에 넣는다. 실패해도 기존 text는 유지한다. */
  async function convertFile(file: File) {
    if (converting) return
    // 파일을 메모리로 읽기 전에 크기 상한을 검사한다.
    if (file.size > MAX_INPUT_BYTES) {
      showErrorMessage(inputTooLargeMessage())
      return
    }
    converting = true
    warnings = []
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      await ensureWasm()
      const result = JSON.parse(convert_hwpx(bytes, file.name)) as {
        markdown: string
        warnings: string[]
      }
      text = result.markdown
      warnings = result.warnings
      sourceName = file.name
    } catch (e) {
      console.error(e)
      showErrorMessage(toUserError(e))
    } finally {
      converting = false
    }
  }

  /** 파일 여러 개 일괄 처리: hwpx만 곧바로 md로 연속 다운로드하고 마지막으로
   *  성공한 파일만 편집기에 남긴다. hwpx가 아니거나 손상된 파일은 조용히 건너뛴다. */
  async function convertMany(files: File[]) {
    if (converting) return
    converting = true
    let last: { name: string; markdown: string; warnings: string[] } | undefined
    try {
      await ensureWasm()
      for (const file of files) {
        try {
          // 크기 초과 파일은 arrayBuffer()로 읽지 않고 다른 실패 파일과
          // 같은 개별 파일 오류 경로(스킵)로 건너뛴다.
          if (file.size > MAX_INPUT_BYTES) {
            console.error(`${inputTooLargeMessage()}: ${file.name}`)
            continue
          }
          const bytes = new Uint8Array(await file.arrayBuffer())
          const result = JSON.parse(convert_hwpx(bytes, file.name)) as {
            markdown: string
            warnings: string[]
          }
          // 브라우저가 연속 다운로드를 무시하지 않도록 사이에 간격을 둔다
          if (last) await new Promise((resolve) => setTimeout(resolve, 300))
          downloadText(result.markdown, buildFilename(file.name))
          last = { name: file.name, markdown: result.markdown, warnings: result.warnings }
        } catch {
          // hwpx가 아닌 파일은 조용히 스킵
        }
      }
    } catch (e) {
      console.error(e)
      showErrorMessage(toUserError(e))
    } finally {
      converting = false
    }
    if (last) {
      text = last.markdown
      warnings = last.warnings
      sourceName = last.name
    }
  }

  // 텍스트영역은 파일 드래그를 항상 변환 대상으로 받는다(변환 결과가 표시된
  // 뒤에도). 텍스트 드래그는 편집기 기본 동작을 유지한다.
  function onTextDragOver(e: DragEvent) {
    const isFileDrag = Array.from(e.dataTransfer?.types ?? []).includes('Files')
    if (!isFileDrag) return
    e.preventDefault()
    textDragOver = true
  }

  function onTextDragLeave() {
    textDragOver = false
  }

  function onTextDrop(e: DragEvent) {
    const isFileDrag = Array.from(e.dataTransfer?.types ?? []).includes('Files')
    if (!isFileDrag) return
    e.preventDefault()
    e.stopPropagation()
    textDragOver = false
    const files = Array.from(e.dataTransfer?.files ?? [])
    if (files.length === 1 && !routeForeignFile(files[0])) void convertFile(files[0])
    else if (files.length > 1) void convertMany(files)
  }

  function onBoxDrop(e: DragEvent) {
    e.preventDefault()
    e.stopPropagation()
    boxDragOver = false
    const files = Array.from(e.dataTransfer?.files ?? [])
    if (files.length === 1 && !routeForeignFile(files[0])) void convertFile(files[0])
    else if (files.length > 1) void convertMany(files)
  }

  function onFilePicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file) void convertFile(file)
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  class="flex h-[var(--vvh,100dvh)] flex-col overflow-hidden bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100"
>
  <!-- 헤더: md2hwpx와 공유하는 스왑 워드마크(앱 이름/링크만 반대) + 테마 -->
  <header
    class="flex shrink-0 items-center justify-between gap-2 border-b border-neutral-200 bg-white px-3 py-2 dark:border-neutral-800 dark:bg-neutral-900 sm:px-4"
  >
    <SwapHeader active="h2m" {onNavigate} />
    <div class="flex shrink-0 items-center gap-2">
      <button
        type="button"
        onclick={() => (previewMode = !previewMode)}
        class="inline-flex h-11 items-center gap-1.5 rounded-lg px-3 text-sm font-medium text-neutral-600 hover:bg-neutral-100 active:bg-neutral-200 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:active:bg-neutral-700"
      >
        {#if previewMode}
          <EyeOff class="size-4" />
          <span>편집</span>
        {:else}
          <Eye class="size-4" />
          <span>미리보기</span>
        {/if}
      </button>
      <!-- [미러 포인트] 인쇄 버튼 — 미리보기 모드에서만 노출한다(원시 마크다운을
           인쇄하는 것은 무의미하므로 편집 모드에는 버튼이 없다). M2hPage도
           토글과 테마 사이 같은 자리·같은 클래스로 둔다. -->
      {#if previewMode}
        <button
          type="button"
          onclick={onPrint}
          title="미리보기를 인쇄합니다 (PDF로 저장 가능)"
          class="inline-flex h-11 items-center gap-1.5 rounded-lg px-3 text-sm font-medium text-neutral-600 hover:bg-neutral-100 active:bg-neutral-200 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:active:bg-neutral-700"
        >
          <Printer class="size-4" />
          <span>인쇄</span>
        </button>
      {/if}
      <button
        type="button"
        onclick={onThemeClick}
        aria-label="테마 전환 (자동/밝게/어둡게)"
        class="inline-flex h-11 items-center gap-1.5 rounded-lg px-3 text-sm font-medium text-neutral-600 hover:bg-neutral-100 active:bg-neutral-200 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:active:bg-neutral-700"
      >
        {#if getTheme() === 'light'}
          <Sun class="size-4" />
        {:else if getTheme() === 'dark'}
          <Moon class="size-4" />
        {:else}
          <SunMoon class="size-4" />
        {/if}
        <span>테마</span>
      </button>
    </div>
  </header>

  <!-- 변환 실패 배너: 편집기 위에 짧게 떴다가 사라진다 -->
  {#if errorMessage}
    <div
      role="alert"
      class="flex shrink-0 items-center justify-center gap-2 border-b border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-400"
    >
      <TriangleAlert class="size-4 shrink-0" />
      <span class="min-w-0 truncate">{errorMessage}</span>
      <button
        type="button"
        onclick={() => (errorMessage = '')}
        aria-label="오류 닫기"
        class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-red-100 dark:hover:bg-red-900/50"
      >
        <X class="size-3.5" />
      </button>
    </div>
  {/if}

  <!-- 변환 결과 편집기(비어 있을 때는 드롭 영역) / 미리보기 -->
  <main class="relative min-h-0 flex-1">
    {#if previewMode}
      <div
        id="print-area"
        class="print-area absolute inset-0 overflow-y-auto px-4 pb-4 pt-4 print:overflow-visible print:p-0 {textDragOver
          ? 'bg-emerald-50 dark:bg-emerald-950/40'
          : ''}"
        ondragover={onTextDragOver}
        ondragleave={onTextDragLeave}
        ondrop={onTextDrop}
      >
        {@html previewHtml}
      </div>
    {:else}
      <textarea
        bind:value={text}
        ondragover={onTextDragOver}
        ondragleave={onTextDragLeave}
        ondrop={onTextDrop}
        spellcheck="false"
        autocomplete="off"
        aria-label="마크다운 변환 결과"
        placeholder="여기에 .hwpx 파일을 끌어다 놓으세요"
        class="absolute inset-0 h-full w-full resize-none overflow-y-auto px-4 pb-4 pr-12 pt-4 font-mono text-sm leading-relaxed outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600 {textDragOver
          ? 'bg-emerald-50 dark:bg-emerald-950/40'
          : 'bg-transparent'}"
      ></textarea>
      {#if text.trim().length > 0}
        <button
          type="button"
          onclick={onCopyButton}
          aria-label={copied ? '복사됨' : '본문 복사'}
          class="absolute right-3 top-3 inline-flex h-11 w-11 items-center justify-center rounded-lg border border-neutral-200 bg-white/90 text-neutral-600 shadow-sm backdrop-blur transition-colors hover:text-neutral-900 dark:border-neutral-700 dark:bg-neutral-900/90 dark:text-neutral-400 dark:hover:text-neutral-100 {copied
            ? 'border-emerald-500/60 text-emerald-600 dark:text-emerald-500'
            : ''}"
        >
          {#if copied}
            <Check class="size-4" />
          {:else}
            <Copy class="size-4" />
          {/if}
        </button>
      {/if}
    {/if}
  </main>

  <!-- 경고 알림: 변환은 됐지만 건너뛴 표가 있는 경우(최신 변환 기준) -->
  {#if warnings.length > 0}
    <div
      class="flex shrink-0 items-center justify-center gap-1.5 bg-amber-50 px-3 py-1.5 text-xs text-amber-700 dark:bg-amber-950/40 dark:text-amber-500"
    >
      <TriangleAlert class="size-3.5 shrink-0" />
      <span class="min-w-0 truncate">
        일부 표를 완전히 변환하지 못했습니다 ({warnings.length}건)
      </span>
      <button
        type="button"
        onclick={() => (warnings = [])}
        aria-label="경고 닫기"
        class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md hover:bg-amber-100 dark:hover:bg-amber-900/50"
      >
        <X class="size-3" />
      </button>
    </div>
  {/if}

  <!-- 하단 제어 바 -->
  <footer
    class="flex shrink-0 items-center justify-between gap-3 border-t border-neutral-200 bg-white px-3 py-2.5 dark:border-neutral-800 dark:bg-neutral-900 sm:px-4"
  >
    {#if text.trim().length > 0}
      <button
        type="button"
        onclick={() => fileInput?.click()}
        ondragover={(e) => {
          e.preventDefault()
          boxDragOver = true
        }}
        ondragleave={() => (boxDragOver = false)}
        ondrop={onBoxDrop}
        disabled={converting}
        title="클릭: 파일 선택 · 드롭: 즉시 변환"
        class="inline-flex h-11 min-w-0 max-w-[45%] items-center gap-1.5 rounded-lg border px-3 text-sm text-neutral-700 transition-colors disabled:cursor-not-allowed disabled:opacity-40 dark:text-neutral-300 {boxDragOver
          ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950/40'
          : 'border-neutral-200 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800'}"
      >
        <Upload class="size-4 shrink-0" />
        <span class="truncate">다른 hwpx 드롭</span>
      </button>
    {:else}
      <span class="min-w-0 flex-1"></span>
    {/if}

    <button
      type="button"
      onclick={onDownload}
      disabled={!canDownload || converting}
      class="inline-flex h-11 shrink-0 items-center gap-1.5 rounded-lg bg-[#78350f] px-4 text-sm font-semibold text-white transition-colors hover:bg-[#92400e] disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-[#78350f] dark:bg-[#fde68a] dark:text-[#78350f] dark:hover:bg-[#fef08a] dark:disabled:hover:bg-[#fde68a]"
    >
      {#if converting}
        <LoaderCircle class="size-4 animate-spin" />
        <span>변환 중…</span>
      {:else}
        <Zap class="size-4" />
        <span>MD 다운로드</span>
      {/if}
    </button>
  </footer>

  <input
    type="file"
    accept=".hwpx"
    class="hidden"
    bind:this={fileInput}
    onchange={onFilePicked}
  />
</div>

<style>
  /* 화면 미리보기 전용 서식. 인쇄 서식은 print.css가 @media print 안에서
     단독으로 담당하므로 여기의 규칙은 반드시 @media screen 안에만 둔다.
     타이포그래피 값은 app.css의 guide-body(안내 오버레이)와 같은 값을
     쓰고, pv-* 구조(글상자·강조·표·불릿)는 Tailwind preflight가 지운
     마커·테두리를 문서 형상이 읽히도록 최소로만 복원한다. */
  @media screen {
    .print-area {
      font-size: 11.75pt;
      line-height: 1.625;
    }
    .print-area :global(h1) {
      margin: 0 0 0.75rem;
      font-size: 17pt;
      font-weight: 800;
      letter-spacing: -0.025em;
      line-height: 1.4;
      text-align: center;
    }
    .print-area :global(h2) {
      margin-top: 1rem;
      font-size: 12.5pt;
      font-weight: 700;
    }
    .print-area :global(h3) {
      margin-top: 0.75rem;
      font-size: 12pt;
      font-weight: 700;
    }
    .print-area :global(p),
    .print-area :global(li) {
      margin-top: 0.25rem;
      color: var(--color-neutral-600);
    }
    .print-area :global(strong) {
      font-weight: 600;
      color: var(--color-neutral-900);
    }
    .print-area :global(ul) {
      margin-top: 0.25rem;
      padding-left: 1.25rem;
      list-style: disc;
    }
    /* list-style-position: outside에서 li padding은 글자만 밀고 마커는 제자리에
       남으므로 margin으로 밀어 마커+글자가 함께 들여 쓰이게 한다. */
    .print-area :global(.pv-depth2) {
      margin-left: 1.25rem;
    }
    .print-area :global(.pv-depth3) {
      margin-left: 2.5rem;
    }
    .print-area :global(.pv-reference) {
      margin-left: 2.5rem;
      font-size: 10pt;
      color: var(--color-neutral-500);
    }
    .print-area :global(.pv-box) {
      margin-top: 0.75rem;
      padding: 0.75rem 1rem;
      border: 1px solid var(--color-neutral-200);
      border-radius: 0.5rem;
    }
    .print-area :global(.pv-box-title) {
      font-weight: 600;
      color: var(--color-neutral-900);
    }
    .print-area :global(.pv-box-items) {
      margin-top: 0.25rem;
      padding-left: 0;
      list-style: none;
    }
    .print-area :global(.pv-highlight) {
      margin-top: 0.75rem;
      padding: 0.75rem 1rem;
      border: 1px solid var(--color-neutral-200);
      border-left: 3px solid var(--color-neutral-400);
      border-radius: 0.5rem;
      background: var(--color-neutral-100);
    }
    .print-area :global(table) {
      margin-top: 0.75rem;
      width: 100%;
      border-collapse: collapse;
      font-size: 11pt;
    }
    .print-area :global(th),
    .print-area :global(td) {
      padding: 0.375rem 0.625rem;
      border: 1px solid var(--color-neutral-300);
      text-align: left;
      vertical-align: top;
      overflow-wrap: break-word;
    }
    .print-area :global(th) {
      font-weight: 700;
      background: var(--color-neutral-100);
    }
    .print-area :global(.pv-banner) {
      margin-top: 1rem;
      padding-top: 0.5rem;
      border-top: 1px solid var(--color-neutral-200);
    }
    .print-area :global(.pv-banner-label) {
      margin-right: 0.375rem;
      font-size: 10pt;
      font-weight: 600;
      color: var(--color-neutral-500);
    }
    .print-area :global(hr) {
      margin: 1rem 0;
      border: 0;
      border-top: 1px solid var(--color-neutral-300);
    }
    :global(.dark) .print-area :global(p),
    :global(.dark) .print-area :global(li) {
      color: var(--color-neutral-400);
    }
    :global(.dark) .print-area :global(strong),
    :global(.dark) .print-area :global(.pv-box-title) {
      color: var(--color-neutral-100);
    }
    :global(.dark) .print-area :global(.pv-reference),
    :global(.dark) .print-area :global(.pv-banner-label) {
      color: var(--color-neutral-400);
    }
    :global(.dark) .print-area :global(.pv-box),
    :global(.dark) .print-area :global(.pv-highlight),
    :global(.dark) .print-area :global(.pv-banner) {
      border-color: var(--color-neutral-800);
    }
    :global(.dark) .print-area :global(.pv-highlight) {
      border-left-color: var(--color-neutral-600);
      background: var(--color-neutral-900);
    }
    :global(.dark) .print-area :global(th),
    :global(.dark) .print-area :global(td) {
      border-color: var(--color-neutral-700);
    }
    :global(.dark) .print-area :global(th) {
      background: var(--color-neutral-800);
    }
    :global(.dark) .print-area :global(hr) {
      border-top-color: var(--color-neutral-700);
    }
  }
</style>
