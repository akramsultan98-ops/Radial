import { defineConfig, devices } from "@playwright/test";

// UI tests run the real React app against the in-browser mock backend
// (`vite --mode mock`). They verify interaction and states, not conversion —
// real conversions are covered by `cargo test`.
export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:1430",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1240, height: 780 } } }],
  webServer: {
    command: "npx vite --mode mock --port 1430 --strictPort",
    url: "http://localhost:1430",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
