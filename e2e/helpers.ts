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
  await expect(page.getByRole("radiogroup", { name: "Actions", exact: true })).toBeVisible();
}

/** A ring segment (operation or format) by id. */
export function segment(page: Page, id: string) {
  return page.locator(`.radial [data-id="${id}"]`);
}

export async function chooseOperation(page: Page, op: string) {
  await segment(page, op).click();
  await expect(page.getByRole("heading", { level: 2, name: new RegExp(`^${op}$`, "i") })).toBeVisible();
}

/** Open Convert (if needed) and pick a target format. */
export async function chooseFormat(page: Page, format: string) {
  if (!(await page.getByRole("radiogroup", { name: "Output format" }).isVisible())) {
    await segment(page, "convert").click();
    await expect(page.getByRole("radiogroup", { name: "Output format" })).toBeVisible();
  }
  await segment(page, format).click();
  await expect(page.getByRole("button", { name: /^Convert to/ })).toBeVisible();
}

/** Drag across the editor layer between two fractional points. */
export async function dragOnImage(page: Page, from: [number, number], to: [number, number]) {
  const box = await page.locator(".editor-layer").boundingBox();
  if (!box) throw new Error("editor layer not visible");
  await page.mouse.move(box.x + box.width * from[0], box.y + box.height * from[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * to[0], box.y + box.height * to[1], { steps: 8 });
  await page.mouse.up();
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
