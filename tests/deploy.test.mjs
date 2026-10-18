import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

test('deployment preparation puts Vercel-equivalent rules at the asset root', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'hwpx-deploy-'))
  try {
    const result = spawnSync(process.execPath, ['scripts/prepare-cloudflare.mjs', dir], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    const redirects = await readFile(join(dir, '_redirects'), 'utf8')
    for (const path of ['/hwpx/compile', '/hwpx/parse']) assert.ok(redirects.includes(`${path} /hwpx/index.html 200`))
    assert.ok(redirects.includes('/hwpx/ /hwpx/compile 302'))
    assert.ok(redirects.includes('/m2h/* /hwpx/compile 302'))
    assert.ok(!redirects.includes('/hwpx/* /hwpx/index.html'))
    // 200 proxy 규칙이 splat 규칙보다 앞에 와야 한다(_redirects는 순서 매칭).
    assert.ok(redirects.indexOf('/hwpx/compile /hwpx/index.html 200') < redirects.indexOf('/m2h/*'))
    assert.ok(redirects.indexOf('/hwpx/parse /hwpx/index.html 200') < redirects.indexOf('/hwpx/h2m'))
    const headers = await readFile(join(dir, '_headers'), 'utf8')
    const vercel = JSON.parse(await readFile('vercel.json', 'utf8'))
    for (const { key, value } of vercel.headers[0].headers) assert.ok(headers.includes(`${key}: ${value}`))
    assert.ok(headers.includes('/hwpx/assets/*\n  ! Cache-Control\n  Cache-Control: public, max-age=31536000, immutable'))
    assert.ok(headers.includes('Cache-Control: public, max-age=0, must-revalidate'))
  } finally { await rm(dir, { recursive: true, force: true }) }
})
