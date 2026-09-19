import { del, get, set } from 'idb-keyval'

const DRAFT_KEY = 'md2hwpx:draft'
const TEMPLATE_KEY = 'md2hwpx:template'
const GUIDE_KEY = 'md2hwpx:guide'
const GUIDE_SNOOZE_MS = 24 * 60 * 60 * 1000

export interface CustomTemplate {
  name: string
  data: Uint8Array
}

export function loadDraft(): string | null {
  try {
    return localStorage.getItem(DRAFT_KEY)
  } catch {
    return null
  }
}

export function saveDraft(text: string): void {
  try {
    localStorage.setItem(DRAFT_KEY, text)
  } catch {
    // 용량 초과 등 저장 실패는 무시
  }
}

export function clearDraft(): void {
  try {
    localStorage.removeItem(DRAFT_KEY)
  } catch {
    // 저장 실패는 무시
  }
}

export async function loadCustomTemplate(): Promise<CustomTemplate | null> {
  try {
    const t = await get<CustomTemplate>(TEMPLATE_KEY)
    return t ?? null
  } catch {
    return null
  }
}

export async function saveCustomTemplate(name: string, data: Uint8Array): Promise<void> {
  await set(TEMPLATE_KEY, { name, data })
}

export async function clearCustomTemplate(): Promise<void> {
  await del(TEMPLATE_KEY)
}

/** 마지막으로 안내를 닫은 지 24시간이 지났으면 다시 표시한다. */
export function shouldShowGuide(): boolean {
  try {
    const at = Number(localStorage.getItem(GUIDE_KEY))
    return !at || Date.now() - at >= GUIDE_SNOOZE_MS
  } catch {
    return true
  }
}

export function dismissGuide(): void {
  try {
    localStorage.setItem(GUIDE_KEY, String(Date.now()))
  } catch {
    // 저장 실패는 무시 (프라이빗 모드 등)
  }
}
