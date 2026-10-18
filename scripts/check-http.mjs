import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const BASE = process.env.SMOKE_BASE_URL || process.argv[2] || 'http://localhost:8788'
const vercel = JSON.parse(readFileSync(new URL('../vercel.json', import.meta.url), 'utf8'))
const htmlHeaders = vercel.headers[0].headers.map(h => h.key + ': ' + h.value)
const assetHeaders = vercel.headers[1].headers[0].value
const notFound = `${BASE}/hwpx/assets/nonexist-abc123xyz.wasm`

const paths = [
  ['/hwpx/compile', 200],
  ['/hwpx/parse', 200],
]
const redirects = [
  ['/', '/hwpx/compile'],
  ['/hwpx', '/hwpx/compile'],
  ['/hwpx/', '/hwpx/compile'],
  ['/hwpx/m2h', '/hwpx/compile'],
  ['/m2h/test', '/hwpx/compile'],
  ['/hwpx/h2m', '/hwpx/parse'],
  ['/h2m', '/hwpx/parse'],
]
const htmlContentType = 'text/html'

for (const [p, expected] of paths) {
  test(`HTTP ${p} → ${expected}`, async () => {
    const res = await fetch(BASE + p)
    assert.equal(res.status, expected, `status ${res.status} for ${p}`)
    if (expected === 200) {
      assert.ok(res.headers.get('content-type')?.includes('html'), `content-type for ${p}: ${res.headers.get('content-type')}`)
    }
  })
}

for (const [from, to] of redirects) {
  test(`redirect ${from} → ${to}`, async () => {
    const res = await fetch(BASE + from, { redirect: 'manual' })
    assert.equal(res.status, 302)
    const loc = res.headers.get('location')
    assert.ok(loc && loc.includes(to.split('/')[1]), `redirect location for ${from}: ${loc}`)
  })
}

test('404 on missing assets', async () => {
  const res = await fetch(notFound)
  assert.equal(res.status, 404)
})

for (const key of Object.keys(htmlHeaders)) {
  const [hKey, hVal] = htmlHeaders[key].split(': ')
  test(`CSP headers on HTML`, async () => {
    const res = await fetch(BASE + '/hwpx/compile')
    assert.equal(res.headers.get(hKey), hVal, `header ${hKey} missing or wrong`)
  })
}
