<script lang="ts">
  import { ArrowLeftRight } from 'lucide-svelte'

  let {
    active,
    onNavigate,
  }: {
    active: 'm2h' | 'h2m'
    onNavigate?: (page: 'm2h' | 'h2m') => void
  } = $props()

  function handleClick(e: MouseEvent, target: 'm2h' | 'h2m') {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
    e.preventDefault()
    onNavigate?.(target)
  }
</script>

<!-- 헤더 스왑 워드마크: MD2HWPX ⇄ HWPX2MD
     활성 쪽은 span(볼드), 비활성 쪽은 링크로 페이지 전환을 유도한다. -->
<div class="flex min-w-0 items-center gap-1.5 text-base tracking-tight select-none">
  {#if active === 'm2h'}
    <span class="font-extrabold text-[#1a365d] dark:text-[#90cdf4]">MD2HWPX</span>
    <a
      href="/hwpx/parse"
      onclick={(e) => handleClick(e, 'h2m')}
      aria-label="HWPX2MD (parse)로 전환"
      class="inline-flex items-center text-neutral-400 transition-colors hover:text-neutral-700 dark:text-neutral-600 dark:hover:text-neutral-300"
    >
      <ArrowLeftRight class="size-3.5" />
    </a>
    <a
      href="/hwpx/parse"
      onclick={(e) => handleClick(e, 'h2m')}
      class="font-medium text-neutral-400 transition-colors hover:text-neutral-900 hover:underline dark:text-neutral-500 dark:hover:text-neutral-100"
    >
      HWPX2MD
    </a>
  {:else}
    <span class="font-extrabold text-[#78350f] dark:text-[#fde68a]">HWPX2MD</span>
    <a
      href="/hwpx/compile"
      onclick={(e) => handleClick(e, 'm2h')}
      aria-label="MD2HWPX (compile)로 전환"
      class="inline-flex items-center text-neutral-400 transition-colors hover:text-neutral-700 dark:text-neutral-600 dark:hover:text-neutral-300"
    >
      <ArrowLeftRight class="size-3.5" />
    </a>
    <a
      href="/hwpx/compile"
      onclick={(e) => handleClick(e, 'm2h')}
      class="font-medium text-neutral-400 transition-colors hover:text-neutral-900 hover:underline dark:text-neutral-500 dark:hover:text-neutral-100"
    >
      MD2HWPX
    </a>
  {/if}
</div>
