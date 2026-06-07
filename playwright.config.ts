import { defineConfig, devices } from "@playwright/test";

// Run with: pnpm test:e2e
// First-time setup: pnpm playwright install --with-deps chromium

export default defineConfig({
  testDir: "tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? "github" : "list",

  webServer: {
    command: "pnpm preview",
    url: "http://localhost:4321",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },

  use: {
    baseURL: "http://localhost:4321",
    trace: "on-first-retry",
  },

  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],

  expect: {
    toMatchSnapshot: { maxDiffPixelRatio: 0.01 },
  },
});
