import { defineConfig } from '@playwright/test'

const baseURL = process.env.SMOKE_BASE_URL ?? 'http://localhost:8788'

export default defineConfig({
  testDir: 'tests/e2e',
  timeout: 60_000,
  retries: 0,
  workers: 1,
  use: {
    baseURL,
    trace: 'retain-on-failure',
  },
  // SMOKE_BASE_URL이 없으면 로컬 wrangler dev를 띄운다(이미 떠 있으면 재사용).
  ...(process.env.SMOKE_BASE_URL
    ? {}
    : {
        webServer: {
          command: 'npx wrangler dev --port 8788',
          url: 'http://localhost:8788/hwpx/compile',
          reuseExistingServer: true,
          timeout: 60_000,
        },
      }),
})
