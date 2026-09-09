<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import {
    Check,
    Copy,
    Download,
    LoaderCircle,
    Moon,
    Package,
    RotateCcw,
    Sparkles,
    Sun,
    SunMoon,
    Upload,
    X,
    Zap,
  } from 'lucide-svelte'
  import SwapHeader from '../components/SwapHeader.svelte'
  import { ensureWasm, compile_hwpx } from '../lib/wasm'
  import { cycleTheme, getTheme } from '../lib/theme.svelte'
  import {
    clearCustomTemplate,
    clearDraft,
    dismissGuide as saveGuideDismissal,
    loadCustomTemplate,
    loadDraft,
    saveCustomTemplate,
    saveDraft,
    shouldShowGuide,
  } from '../lib/storage'
  import { buildM2hFilename as buildFilename, downloadBytes } from '../lib/download'
  import { copyText } from '../lib/clipboard'
  import { renderInstructionMarkdown } from '../lib/instruction'

  let { onNavigate }: { onNavigate?: (page: 'm2h' | 'h2m') => void } = $props()

  const DEFAULT_TEMPLATE_NAME = 'template.hwpx'

  let text = $state('')
  let promptText = $state('')
  let instructionHtml = $state('')
  let exampleText = $state('')

  let templateName = $state(DEFAULT_TEMPLATE_NAME)
  let hasCustom = $state(false)

  let showTemplateSheet = $state(false)
  let showPromptSheet = $state(false)
  let showGuide = $state(false)
  let boxDragOver = $state(false)
  let sheetDragOver = $state(false)
  let editorTextDragOver = $state(false)

  let copied = $state(false)
  let promptCopied = $state(false)
  let downloading = $state(false)

  let fileInput = $state<HTMLInputElement>()

  let defaultTemplateBytes: Uint8Array | null = null
  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let promptCopyTimer: ReturnType<typeof setTimeout> | undefined
  let saveTimer: ReturnType<typeof setTimeout> | undefined

  const canDownload = $derived(
    text.trim().length > 0 && !(promptText !== '' && text === promptText),
  )
  const filenamePreview = $derived(buildFilename(text))

  onMount(() => {
    showGuide = shouldShowGuide()

    // 빈 문자열 초안(구 버전에서 전체 삭제 시 저장된 값)은 초안 없음으로 취급한다.
    const draft = loadDraft()
    if (draft) {
      text = draft
    }

    fetch(import.meta.env.BASE_URL + 'prompt.md')
      .then((r) => {
        if (!r.ok) throw new Error(`prompt.md 요청 실패 (${r.status})`)
        return r.text()
      })
      .then((raw) => {
        promptText = raw
      })
      .catch((e) => console.error('프롬프트 로드 실패:', e))

    // 첫 방문(저장된 초안 없음)이면 예시 문서로 편집기를 채운다.
    fetch(import.meta.env.BASE_URL + 'example.md')
      .then((r) => {
        if (!r.ok) throw new Error(`example.md 요청 실패 (${r.status})`)
        return r.text()
      })
      .then((t) => {
        exampleText = t
        if (!draft && text === '') text = t
      })
      .catch((e) => console.error('예시 로드 실패:', e))

    fetch(import.meta.env.BASE_URL + 'instruction.md')
      .then((r) => {
        if (!r.ok) throw new Error(`instruction.md 요청 실패 (${r.status})`)
        return r.text()
      })
      .then((md) => {
        instructionHtml = renderInstructionMarkdown(md)
      })
      .catch((e) => console.error('안내 로드 실패:', e))

    void loadCustomTemplate().then((t) => {
      if (t) {
        templateName = t.name
        hasCustom = true
      }
    })

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

  function scheduleSave() {
    clearTimeout(saveTimer)
    // 내용을 완전히 지우면 초안을 버리고 예시 문서로 되돌린다.
    if (text === '' && exampleText) {
      clearDraft()
      text = exampleText
      return
    }
    saveTimer = setTimeout(() => saveDraft(text), 400)
  }

  function onGuideDismiss() {
    saveGuideDismissal()
    showGuide = false
  }

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

  async function onPromptCopy() {
    if (!promptText) return
    if (await copyText(promptText)) {
      promptCopied = true
      clearTimeout(promptCopyTimer)
      promptCopyTimer = setTimeout(() => (promptCopied = false), 1500)
    }
  }

  async function fetchDefaultTemplate(): Promise<Uint8Array> {
    if (defaultTemplateBytes) return defaultTemplateBytes
    const res = await fetch(import.meta.env.BASE_URL + 'template.hwpx')
    if (!res.ok) throw new Error(`template.hwpx 요청 실패 (${res.status})`)
    defaultTemplateBytes = new Uint8Array(await res.arrayBuffer())
    return defaultTemplateBytes
  }

  async function onDownload() {
    if (!canDownload || downloading) return
    downloading = true
    try {
      await ensureWasm()
      const custom = hasCustom ? await loadCustomTemplate() : null
      const templateBytes = custom?.data ?? (await fetchDefaultTemplate())
      const output = compile_hwpx(text, templateBytes)
      downloadBytes(output, buildFilename(text))
    } catch (e) {
      console.error(e)
      alert(`HWPX 변환 실패: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      downloading = false
    }
  }

  async function downloadDefaultTemplate() {
    try {
      const bytes = await fetchDefaultTemplate()
      downloadBytes(bytes, DEFAULT_TEMPLATE_NAME)
    } catch (e) {
      console.error(e)
      alert('기본 템플릿 다운로드에 실패했습니다.')
    }
  }

  async function applyTemplateFile(file: File): Promise<boolean> {
    if (!file.name.toLowerCase().endsWith('.hwpx')) {
      alert('.hwpx 파일만 템플릿으로 등록할 수 있습니다.')
      return false
    }
    const data = new Uint8Array(await file.arrayBuffer())
    await saveCustomTemplate(file.name, data)
    templateName = file.name
    hasCustom = true
    return true
  }

  async function resetTemplate() {
    await clearCustomTemplate()
    templateName = DEFAULT_TEMPLATE_NAME
    hasCustom = false
  }

  function onBoxDrop(e: DragEvent) {
    e.preventDefault()
    e.stopPropagation()
    boxDragOver = false
    const file = e.dataTransfer?.files?.[0]
    if (file) void applyTemplateFile(file)
  }

  function onSheetDrop(e: DragEvent) {
    e.preventDefault()
    e.stopPropagation()
    sheetDragOver = false
    const file = e.dataTransfer?.files?.[0]
    if (file) {
      void applyTemplateFile(file).then((ok) => {
        if (ok) showTemplateSheet = false
      })
    }
  }

  function onFilePicked(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file) {
      void applyTemplateFile(file).then((ok) => {
        if (ok) showTemplateSheet = false
      })
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      showTemplateSheet = false
      showPromptSheet = false
    }
  }

  // 텍스트영역은 .md/.txt 파일 드래그를 받는다. 텍스트 드래그는 편집기 기본
  // 동작을 유지하고, 파일 2개 이상·다른 확장자는 조용히 무시한다.
  function onEditorDragOver(e: DragEvent) {
    const isFileDrag = Array.from(e.dataTransfer?.types ?? []).includes('Files')
    if (!isFileDrag) return
    e.preventDefault()
    editorTextDragOver = true
  }

  function onEditorDragLeave() {
    editorTextDragOver = false
  }

  function onEditorDrop(e: DragEvent) {
    const isFileDrag = Array.from(e.dataTransfer?.types ?? []).includes('Files')
    if (!isFileDrag) return
    e.preventDefault()
    e.stopPropagation()
    editorTextDragOver = false
    const files = Array.from(e.dataTransfer?.files ?? [])
    if (files.length !== 1) return // 파일 1개만 지원
    const file = files[0]
    const name = file.name.toLowerCase()
    if (!name.endsWith('.md') && !name.endsWith('.txt')) return
    void file
      .text()
      .then((content) => {
        text = content
        scheduleSave()
      })
      .catch((err) => console.error(err))
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- 최초 방문 안내 오버레이: 아무 곳이나 누르면 닫히고 24시간 동안 숨겨진다.
     본문은 public/instruction.md를 렌더링한 것. -->
{#if showGuide && instructionHtml}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm"
    transition:fade={{ duration: 150 }}
    onclick={onGuideDismiss}
  >
    <div
      class="guide-body max-h-[85dvh] w-full max-w-md overflow-y-auto rounded-2xl bg-white p-5 shadow-xl dark:bg-neutral-900"
    >
      {@html instructionHtml}
    </div>
  </div>
{/if}

<div
  class="flex h-[var(--vvh,100dvh)] flex-col overflow-hidden bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100"
>
  <!-- 헤더 -->
  <header
    class="flex shrink-0 items-center justify-between gap-2 border-b border-neutral-200 bg-white px-3 py-2 dark:border-neutral-800 dark:bg-neutral-900 sm:px-4"
  >
    <SwapHeader active="m2h" {onNavigate} />
    <div class="flex shrink-0 items-center gap-2">
      <button
        type="button"
        onclick={() => {
          showTemplateSheet = false
          showPromptSheet = true
        }}
        class="inline-flex h-11 items-center gap-1.5 rounded-lg px-3 text-sm font-medium text-neutral-600 hover:bg-neutral-100 active:bg-neutral-200 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:active:bg-neutral-700"
      >
        <Sparkles class="size-4" />
        <span>지침</span>
      </button>
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

  <!-- 에디터 -->
  <main class="relative min-h-0 flex-1">
    <textarea
      bind:value={text}
      oninput={scheduleSave}
      ondragover={onEditorDragOver}
      ondragleave={onEditorDragLeave}
      ondrop={onEditorDrop}
      spellcheck="false"
      autocomplete="off"
      aria-label="마크다운 에디터"
      placeholder="여기에 마크다운을 붙여넣거나 .md/.txt 파일을 끌어다 놓으세요"
      class="absolute inset-0 h-full w-full resize-none overflow-y-auto px-4 pb-4 pr-12 pt-4 font-mono text-sm leading-relaxed outline-none placeholder:text-neutral-400 dark:placeholder:text-neutral-600 {editorTextDragOver
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

  <!-- 하단 제어 바 -->
  <footer
    class="flex shrink-0 items-center justify-between gap-3 border-t border-neutral-200 bg-white px-3 py-2.5 dark:border-neutral-800 dark:bg-neutral-900 sm:px-4"
  >
    <button
      type="button"
      onclick={() => (showTemplateSheet = true)}
      ondragover={(e) => {
        e.preventDefault()
        boxDragOver = true
      }}
      ondragleave={() => (boxDragOver = false)}
      ondrop={onBoxDrop}
      title="클릭: 템플릿 관리 · 드롭: 즉시 교체"
      class="inline-flex h-11 min-w-0 max-w-[45%] items-center gap-1.5 rounded-lg border px-3 text-sm text-neutral-700 transition-colors dark:text-neutral-300 {boxDragOver
        ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950/40'
        : 'border-neutral-200 hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800'}"
    >
      <Package class="size-4 shrink-0" />
      <span class="truncate">{templateName}</span>
    </button>

    <button
      type="button"
      onclick={onDownload}
      disabled={!canDownload || downloading}
      class="inline-flex h-11 shrink-0 items-center gap-1.5 rounded-lg bg-[#1a365d] px-4 text-sm font-semibold text-white transition-colors hover:bg-[#2a4365] disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-[#1a365d] dark:bg-[#90cdf4] dark:text-[#1a365d] dark:hover:bg-[#bee3f8] dark:disabled:hover:bg-[#90cdf4]"
    >
      {#if downloading}
        <LoaderCircle class="size-4 animate-spin" />
        <span>변환 중…</span>
      {:else}
        <Zap class="size-4" />
        <span>HWPX 다운로드</span>
      {/if}
    </button>
  </footer>
</div>

<!-- 템플릿 관리 시트 -->
{#if showTemplateSheet}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40 flex items-end justify-center bg-black/50 sm:items-center"
    transition:fade={{ duration: 150 }}
    onclick={(e) => {
      if (e.target === e.currentTarget) showTemplateSheet = false
    }}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label="템플릿 관리"
      tabindex="-1"
      transition:fly={{ y: 32, duration: 180 }}
      class="flex max-h-[85dvh] w-full flex-col rounded-t-2xl bg-white shadow-xl dark:bg-neutral-900 sm:max-w-lg sm:rounded-2xl"
    >
      <div
        class="flex shrink-0 items-center justify-between gap-2 border-b border-neutral-200 px-4 py-2 dark:border-neutral-800"
      >
        <div class="flex min-w-0 items-center gap-2">
          <Package class="size-5 shrink-0" />
          <span class="font-semibold">템플릿 관리 가이드</span>
        </div>
        <button
          type="button"
          onclick={() => (showTemplateSheet = false)}
          aria-label="닫기"
          class="inline-flex h-11 w-11 items-center justify-center rounded-lg text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
        >
          <X class="size-5" />
        </button>
      </div>

      <div class="space-y-5 overflow-y-auto px-4 py-4 text-sm">
        <p
          class="rounded-lg bg-neutral-100 px-3 py-2 text-xs text-neutral-600 dark:bg-neutral-800/60 dark:text-neutral-400"
        >
          현재 템플릿: <span class="font-medium text-neutral-900 dark:text-neutral-100"
            >{templateName}</span
          >
        </p>

        <section class="space-y-1.5">
          <h3 class="font-semibold">1. 샘플 양식 받기</h3>
          <p class="text-[13px] text-neutral-500 dark:text-neutral-400">
            기본 템플릿을 받아 한컴오피스에서 엽니다.
          </p>
          <button
            type="button"
            onclick={downloadDefaultTemplate}
            class="mt-1 inline-flex h-11 items-center gap-1.5 rounded-lg border border-neutral-300 px-3 text-[13px] font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-800"
          >
            <Download class="size-4" />
            <span>기본 template.hwpx 다운로드</span>
          </button>
        </section>

        <section class="space-y-1.5">
          <h3 class="font-semibold">2. 자유롭게 서식 수정</h3>
          <p class="text-[13px] leading-relaxed text-neutral-500 dark:text-neutral-400">
            메모 표식([[카테고리]])만 그대로 두고, 로고, 글꼴, 표 테두리, 배경색을 원하는 대로
            고칩니다.
          </p>
        </section>

        <section class="space-y-1.5">
          <h3 class="font-semibold">3. 새 템플릿 등록</h3>
          <p class="text-[13px] text-neutral-500 dark:text-neutral-400">
            수정한 파일을 이 창에 끌어다 놓거나 선택하세요.
          </p>
          <button
            type="button"
            onclick={() => fileInput?.click()}
            ondragover={(e) => {
              e.preventDefault()
              sheetDragOver = true
            }}
            ondragleave={() => (sheetDragOver = false)}
            ondrop={onSheetDrop}
            class="mt-1 flex h-20 w-full flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed text-[13px] transition-colors {sheetDragOver
              ? 'border-emerald-500 bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-500'
              : 'border-neutral-300 text-neutral-500 hover:border-neutral-400 dark:border-neutral-700 dark:text-neutral-400 dark:hover:border-neutral-600'}"
          >
            <Upload class="size-5" />
            <span>새 .hwpx 파일 선택 (또는 드롭)</span>
          </button>
          <input
            type="file"
            accept=".hwpx"
            class="hidden"
            bind:this={fileInput}
            onchange={onFilePicked}
          />
        </section>

        {#if hasCustom}
          <button
            type="button"
            onclick={resetTemplate}
            class="inline-flex h-11 w-full items-center justify-center gap-1.5 rounded-lg border border-neutral-300 text-[13px] font-medium text-neutral-600 hover:bg-neutral-100 dark:border-neutral-700 dark:text-neutral-400 dark:hover:bg-neutral-800"
          >
            <RotateCcw class="size-4" />
            <span>기본 템플릿으로 초기화</span>
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- 프롬프트 열람 시트 -->
{#if showPromptSheet}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40 flex items-end justify-center bg-black/50 sm:items-center"
    transition:fade={{ duration: 150 }}
    onclick={(e) => {
      if (e.target === e.currentTarget) showPromptSheet = false
    }}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label="마크다운 변환 지침 프롬프트"
      tabindex="-1"
      transition:fly={{ y: 32, duration: 180 }}
      class="flex max-h-[85dvh] w-full flex-col rounded-t-2xl bg-white shadow-xl dark:bg-neutral-900 sm:max-w-xl sm:rounded-2xl"
    >
      <div
        class="flex shrink-0 items-center justify-between gap-2 border-b border-neutral-200 px-4 py-2 dark:border-neutral-800"
      >
        <div class="flex min-w-0 items-center gap-2">
          <Copy class="size-5 shrink-0" />
          <span class="font-semibold">마크다운 변환 지침 프롬프트</span>
        </div>
        <button
          type="button"
          onclick={() => (showPromptSheet = false)}
          aria-label="닫기"
          class="inline-flex h-11 w-11 items-center justify-center rounded-lg text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
        >
          <X class="size-5" />
        </button>
      </div>

      <div class="shrink-0 px-4 pt-3">
        <button
          type="button"
          onclick={onPromptCopy}
          class="flex h-12 w-full items-center justify-center gap-2 rounded-xl bg-neutral-900 text-sm font-semibold text-white transition-colors hover:bg-neutral-700 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-200 {promptCopied
            ? 'bg-emerald-600 hover:bg-emerald-600 dark:bg-emerald-600 dark:hover:bg-emerald-600 dark:text-white'
            : ''}"
        >
          {#if promptCopied}
            <Check class="size-4" />
            <span>복사됨</span>
          {:else}
            <Copy class="size-4" />
            <span>프롬프트 복사하기</span>
          {/if}
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <pre
          class="whitespace-pre-wrap break-words font-mono text-[13px] leading-relaxed text-neutral-700 dark:text-neutral-300">{promptText}</pre>
      </div>
    </div>
  </div>
{/if}
