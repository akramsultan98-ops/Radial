import { expect, type Page } from "@playwright/test";

export const SHOTS = "qa/screenshots/ui";

export async function shot(page: Page, name: string) {
  await page.waitForTimeout(350); // let entrance animations settle
  await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

/** Load the app and wait until the backend is connected. */
export async function ready(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("list", { name: "Formats RADIAL can open" })).toBeVisible();
}

export async function openSample(page: Page) {
  await ready(page);
  await page.getByRole("button", { name: "Choose file" }).click();
  await expect(page.getByRole("radiogroup", { name: "Output format" })).toBeVisible();
}

export function segment(page: Page, format: string) {
  return page.locator(`.radial [data-format="${format}"]`);
}

export async function chooseFormat(page: Page, format: string) {
  await segment(page, format).click();
  await expect(page.getByRole("button", { name: /^Convert to/ })).toBeVisible();
}

/** Speed up (or slow down) the mock's simulated stages. */
export async function setStageDelay(page: Page, ms: number) {
  await page.evaluate((v) => {
    if (window.__radialMock) window.__radialMock.stageDelay = v;
  }, ms);
}

export async function failNext(page: Page, kind: string, message: string) {
  await page.evaluate(
    ([k, m]) => {
      if (window.__radialMock) window.__radialMock.failNext = { kind: k as never, message: m ?? "", retryable: true };
    },
    [kind, message],
  );
}
