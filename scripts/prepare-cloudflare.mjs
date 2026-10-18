import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
const destination = process.argv[2] ?? 'web/dist'
const vercel = JSON.parse(await readFile(new URL('../vercel.json', import.meta.url), 'utf8'))
await mkdir(destination, { recursive: true })
const redirects = [
  // 명시적 200 proxy(두 화면)를 splat 규칙보다 먼저 둔다. _redirects는 순서 매칭이다.
  '/hwpx/compile /hwpx/index.html 200',
  '/hwpx/parse /hwpx/index.html 200',
  ...['/', '/hwpx', '/hwpx/', '/hwpx/m2h', '/m2h', '/m2h/*'].map(path => `${path} /hwpx/compile 302`),
  ...['/hwpx/h2m', '/h2m', '/h2m/*'].map(path => `${path} /hwpx/parse 302`),
].join('\n') + '\n'
const headers = '/*\n' + vercel.headers[0].headers.map(({ key, value }) => `  ${key}: ${value}`).join('\n') +
  '\n  Cache-Control: public, max-age=0, must-revalidate\n\n/hwpx/assets/*\n  ! Cache-Control\n  Cache-Control: public, max-age=31536000, immutable\n'
await writeFile(join(destination, '_redirects'), redirects)
await writeFile(join(destination, '_headers'), headers)
