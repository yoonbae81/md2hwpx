export type ThemeMode = 'auto' | 'light' | 'dark'

// 모듈 $state: 두 페이지(compile/parse)가 getTheme()으로 같은 상태를 반응형으로 읽는다.
const THEME_KEY = 'md2hwpx:theme'
const ORDER: ThemeMode[] = ['auto', 'light', 'dark']

let current = $state<ThemeMode>('auto')
let initialized = false

function readStored(): ThemeMode {
  try {
    const v = localStorage.getItem(THEME_KEY)
    return v === 'auto' || v === 'light' || v === 'dark' ? v : 'auto'
  } catch {
    return 'auto'
  }
}

function systemPrefersDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

export function resolvedDark(mode: ThemeMode = current): boolean {
  return mode === 'dark' || (mode === 'auto' && systemPrefersDark())
}

function applyTheme(): void {
  document.documentElement.classList.toggle('dark', resolvedDark())
}

export function getTheme(): ThemeMode {
  return current
}

export function setTheme(mode: ThemeMode): void {
  current = mode
  try {
    localStorage.setItem(THEME_KEY, mode)
  } catch {
    // 저장 실패는 무시 (프라이빗 모드 등)
  }
  applyTheme()
}

export function cycleTheme(): ThemeMode {
  const next = ORDER[(ORDER.indexOf(current) + 1) % ORDER.length]
  setTheme(next)
  return next
}

export function initTheme(): void {
  if (initialized) return
  initialized = true
  current = readStored()
  applyTheme()
  window
    .matchMedia('(prefers-color-scheme: dark)')
    .addEventListener('change', () => {
      if (current === 'auto') applyTheme()
    })
}
