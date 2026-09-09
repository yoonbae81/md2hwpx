<script lang="ts">
  import { onMount } from 'svelte'
  import {
    Check,
    Copy,
    LoaderCircle,
    Moon,
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

  let { onNavigate }: { onNavigate?: (page: 'm2h' | 'h2m') => void } = $props()

  let text = $state('')

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
  const downloadName = $derived(sourceName !== '' ? buildFilename(sourceName) : 'converted.md')

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

  /** 파일 하나를 변환해 편집기에 넣는다. 실패해도 기존 text는 유지한다. */
  async function convertFile(file: File) {
    if (converting) return
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
    const file = e.dataTransfer?.files?.[0]
    if (file) void convertFile(file)
  }

  function onBoxDrop(e: DragEvent) {
    e.preventDefault()
    e.stopPropagation()
    boxDragOver = false
    const file = e.dataTransfer?.files?.[0]
    if (file) void convertFile(file)
  }

  function onFilePicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file) void convertFile(file)
  }
</script>

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

  <!-- 변환 결과 편집기(비어 있을 때는 드롭 영역) -->
  <main class="relative min-h-0 flex-1">
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
