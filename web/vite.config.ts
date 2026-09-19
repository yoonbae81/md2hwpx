import { defineConfig, type Plugin } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import type { IncomingMessage, ServerResponse } from 'node:http'

function redirectMiddleware(req: IncomingMessage, res: ServerResponse, next: () => void) {
  const url = req.url?.split('?')[0] ?? ''
  if (url === '/' || url === '/hwpx' || url === '/hwpx/' || url === '/m2h' || url === '/hwpx/m2h') {
    res.writeHead(302, { Location: '/hwpx/compile' }).end()
    return
  }
  if (url === '/h2m' || url === '/hwpx/h2m') {
    res.writeHead(302, { Location: '/hwpx/parse' }).end()
    return
  }
  next()
}

function hwpxRedirectPlugin(): Plugin {
  return {
    name: 'hwpx-redirect',
    configureServer(server) {
      server.middlewares.use(redirectMiddleware)
    },
    configurePreviewServer(server) {
      server.middlewares.use(redirectMiddleware)
    },
  }
}

export default defineConfig({
  base: '/hwpx/',
  build: {
    // 자산·public 파일이 base 경로(/hwpx/) 그대로 서빙되도록 산출물을 dist/hwpx에 놓는다.
    // dist 루트에 두면 vercel.json의 SPA rewrite가 /hwpx/assets/*를 index.html로 삼켜 흰 화면이 된다.
    outDir: 'dist/hwpx',
  },
  plugins: [hwpxRedirectPlugin(), svelte(), tailwindcss()],
  server: {
    port: 8292,
    strictPort: true,
    host: '0.0.0.0',
  },
  preview: {
    port: 8292,
    strictPort: true,
    host: '0.0.0.0',
  },
})
