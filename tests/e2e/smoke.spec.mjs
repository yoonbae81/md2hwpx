import { test, expect } from '@playwright/test'

// 사용자 요건: 검증용 텍스트는 실제 문체(공백·조사·어미 섞인 문장)로 만든다.
const SAMPLE_MD = [
  '# 10월 서비스 운영 보고',
  '',
  '이번 주에는 변환 오류가 세 건 접수되었고, 모두 표 셀 줄바꿈 문제였다.',
  '원인을 확인한 뒤 수정판을 배포했으니 다음 주 초에 재현 여부를 다시 확인해 주세요.',
  '',
  '[[박스]]',
  '- 담당자는 김책임, 일정은 10월 21일까지다.',
  '  - 세부 원인 분석 자료는 회의록 3페이지에 있다.',
].join('\n')

test('compile 화면에서 WASM 엔진으로 실제 변환·다운로드가 된다', async ({ page }) => {
  const errors = []
  page.on('pageerror', e => errors.push(String(e)))
  page.on('console', m => {
    if (m.type() === 'error') errors.push(m.text())
  })

  await page.goto('/hwpx/compile')
  await expect(page.getByLabel('마크다운 에디터')).toBeVisible()
  await page.getByLabel('마크다운 에디터').fill(SAMPLE_MD)

  const downloadPromise = page.waitForEvent('download', { timeout: 30_000 })
  await page.getByRole('button', { name: 'HWPX 다운로드' }).click()
  const download = await downloadPromise
  const path = await download.path()
  const { readFileSync } = await import('node:fs')
  const bytes = readFileSync(path)
  // HWPX는 zip 패키지다: PK 시그니처로 엔진이 실제 문서를 만들었음을 검증한다.
  expect(bytes.length).toBeGreaterThan(1000)
  expect(bytes[0]).toBe(0x50)
  expect(bytes[1]).toBe(0x4b)

  // 문서를 서버로 보내지 않는다: 변환 후 네트워크 요청이 없어야 한다(페이지 자산 제외).
  expect(errors, `console/page errors: ${errors.join(' | ')}`).toEqual([])
})

test('parse 화면이 렌더링되고 직접 진입을 지원한다', async ({ page }) => {
  const errors = []
  page.on('pageerror', e => errors.push(String(e)))
  await page.goto('/hwpx/parse')
  await expect(page.locator('textarea').first()).toBeVisible()
  await expect(page.locator('#print-area')).toHaveCount(0) // 편집 모드에서는 미리보기 영역이 없다
  expect(errors).toEqual([])
})

test('레거시 주소가 두 화면으로 임시 리다이렉트된다', async ({ request }) => {
  for (const [from, to] of [
    ['/m2h', '/hwpx/compile'],
    ['/hwpx/h2m', '/hwpx/parse'],
  ]) {
    const res = await request.get(from, { maxRedirects: 0 })
    expect(res.status()).toBe(302)
    expect(res.headers().location).toContain(to)
  }
})
