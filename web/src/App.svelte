<script lang="ts">
  import { onMount } from 'svelte'
  import M2hPage from './pages/M2hPage.svelte'
  import H2mPage from './pages/H2mPage.svelte'

  type Page = 'm2h' | 'h2m'

  function getInitialPage(): Page {
    if (typeof window === 'undefined') return 'm2h'
    const path = window.location.pathname.toLowerCase()
    const hash = window.location.hash.toLowerCase()
    if (path.includes('parse') || hash.includes('parse') || path.includes('h2m') || hash.includes('h2m')) {
      return 'h2m'
    }
    return 'm2h'
  }

  let currentPage = $state<Page>(getInitialPage())

  function updateDocumentMeta(page: Page) {
    if (typeof document === 'undefined') return
    if (page === 'h2m') {
      document.title = 'HWPX2MD — Parse'
    } else {
      document.title = 'MD2HWPX — Compile'
    }
  }

  function navigateTo(page: Page) {
    if (currentPage === page) return
    currentPage = page
    updateDocumentMeta(page)
    const newPath = page === 'h2m' ? '/hwpx/parse' : '/hwpx/compile'
    window.history.pushState({ page }, '', newPath)
  }

  onMount(() => {
    updateDocumentMeta(currentPage)

    // 접속 주소 정규화 (루트나 레거시 경로 진입 시 정식 경로로 URL 치환)
    const path = window.location.pathname.toLowerCase()
    if (path === '/' || path === '/hwpx' || path === '/hwpx/' || path === '/m2h' || path === '/hwpx/m2h') {
      window.history.replaceState({ page: 'm2h' }, '', '/hwpx/compile')
    } else if (path === '/h2m' || path === '/hwpx/h2m') {
      window.history.replaceState({ page: 'h2m' }, '', '/hwpx/parse')
    }

    const onPopState = () => {
      const page = getInitialPage()
      currentPage = page
      updateDocumentMeta(page)
    }

    window.addEventListener('popstate', onPopState)
    return () => {
      window.removeEventListener('popstate', onPopState)
    }
  })
</script>

<div class="h-full w-full">
  <div class="h-full w-full" class:hidden={currentPage !== 'm2h'}>
    <M2hPage onNavigate={navigateTo} />
  </div>
  <div class="h-full w-full" class:hidden={currentPage !== 'h2m'}>
    <H2mPage onNavigate={navigateTo} />
  </div>
</div>
