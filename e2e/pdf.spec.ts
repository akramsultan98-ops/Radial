import { expect, test } from "@playwright/test";
import { chooseFormat, chooseOperation, dragOnImage, openPdf, segment, setStageDelay, shot } from "./helpers";

const previewPages = (page: import("@playwright/test").Page) =>
  page.evaluate(() => window.__radialMock?.previewPages.slice() ?? []);

test.describe("PDF files", () => {
  test.beforeEach(async ({ page }) => {
    await openPdf(page);
    await setStageDelay(page, 30);
  });

  test("shows pages and offers only what works for PDFs", async ({ page }) => {
    const details = page.getByRole("region", { name: "Quarterly report.pdf" });
    await expect(details.getByText("Pages")).toBeVisible();
    await expect(details.getByText("210 × 297 mm")).toBeVisible();
    await expect(page.locator(".orb__meta")).toContainText("3 pages");
    for (const id of ["convert", "compress", "crop", "annotate", "redact", "metadata"]) {
      await expect(segment(page, id)).not.toHaveAttribute("aria-disabled", "true");
    }
    for (const id of ["edit", "background"]) {
      await expect(segment(page, id)).toHaveAttribute("aria-disabled", "true");
    }
    await shot(page, "40-pdf-actions");
    await segment(page, "convert").click();
    const ring = page.getByRole("radiogroup", { name: "Output format" });
    await expect(ring.getByRole("radio")).toHaveCount(3);
    for (const f of ["png", "jpeg", "txt"]) await expect(segment(page, f)).toBeVisible();
  });

  test("redact: mark areas on two pages, add text to find, save", async ({ page }) => {
    await chooseOperation(page, "redact");
    const save = page.getByRole("button", { name: /Redact & save copy/ });
    await expect(save).toBeDisabled();
    await expect(page.getByText("Mark an area or enter text to remove.")).toBeVisible();
    await expect(page.getByText("Page 1 of 3", { exact: true })).toBeVisible();

    await dragOnImage(page, [0.1, 0.2], [0.6, 0.26]);
    await page.getByRole("button", { name: "Next page" }).click();
    await expect(page.getByText("Page 2 of 3", { exact: true })).toBeVisible();
    // The editor shows only this page's areas.
    await expect(page.locator(".redact-box")).toHaveCount(0);
    await dragOnImage(page, [0.3, 0.3], [0.5, 0.4]);
    await expect(page.locator(".redact-box")).toHaveCount(1);
    expect(await previewPages(page)).toContain(1);

    await page.getByLabel("Find and remove text").fill("ACME Corporation\nDE89 3704");
    await expect(page.getByText("2 areas on 2 pages")).toBeVisible();
    await page.getByRole("button", { name: "Area 1 · page 1" }).click();
    await expect(page.getByText("Page 1 of 3", { exact: true })).toBeVisible();
    await shot(page, "41-pdf-redact");

    await expect(save).toBeEnabled();
    await save.click();
    await expect(page.getByRole("heading", { name: /Redacted/ })).toBeVisible();
    await expect(page.getByText(/those pages are now images/)).toBeVisible();
    await shot(page, "42-pdf-redact-done");
  });

  test("redact with only search text is allowed", async ({ page }) => {
    await chooseOperation(page, "redact");
    await page.getByLabel("Find and remove text").fill("ZEBRA-42");
    await expect(page.getByRole("button", { name: /Redact & save copy/ })).toBeEnabled();
  });

  test("convert a page to JPEG with page and resolution", async ({ page }) => {
    await chooseFormat(page, "jpeg");
    await page.getByLabel("Page").fill("2");
    await page.getByRole("radio", { name: "Print" }).click();
    await expect(page.getByText("300 DPI")).toBeVisible();
    await expect(page.getByText(/Quality/)).toBeVisible();
    await shot(page, "43-pdf-to-jpeg");
    await page.getByRole("button", { name: /^Convert to JPEG/ }).click();
    await expect(page.getByRole("heading", { name: /Converted|Saved/ })).toBeVisible();
  });

  test("extract text shows a text result", async ({ page }) => {
    await chooseFormat(page, "txt");
    await expect(page.getByLabel("File name")).toHaveValue("Quarterly report");
    await page.getByRole("button", { name: /^Convert to Text/ }).click();
    await expect(page.getByRole("heading", { name: "Text extracted" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Open text file" })).toBeVisible();
  });

  test("crop can apply to every page", async ({ page }) => {
    await chooseOperation(page, "crop");
    await expect(page.getByRole("button", { name: "Rotate left" })).toHaveCount(0);
    const all = page.getByRole("switch", { name: /Apply to all pages/ });
    await expect(all).toHaveAttribute("aria-checked", "false");
    await expect(page.getByText("Only page 1")).toBeVisible();
    await all.click();
    await expect(page.getByText("Same area on all 3 pages")).toBeVisible();
    await expect(page.getByText(/hides the rest of the page/)).toBeVisible();
    await page.getByRole("button", { name: /Save cropped copy/ }).click();
    await expect(page.getByRole("heading", { name: /Cropped/ })).toBeVisible();
  });

  test("annotations belong to the page they were drawn on", async ({ page }) => {
    await chooseOperation(page, "annotate");
    await dragOnImage(page, [0.2, 0.3], [0.5, 0.5]);
    await expect(page.locator(".editor-layer line, .editor-layer polygon")).not.toHaveCount(0);
    await page.getByRole("button", { name: "Next page" }).click();
    await expect(page.locator(".editor-layer line")).toHaveCount(0);
    await page.getByRole("button", { name: "Previous page" }).click();
    await expect(page.locator(".editor-layer line")).not.toHaveCount(0);
  });
});

test("restricted PDFs are view-only", async ({ page }) => {
  await openPdf(page, { name: "Locked.pdf", restricted: true });
  await expect(page.getByText(/owner password that restricts changes/)).toBeVisible();
  for (const id of ["convert", "compress", "crop", "annotate", "redact"]) {
    await expect(segment(page, id)).toHaveAttribute("aria-disabled", "true");
  }
  await chooseOperation(page, "metadata");
  await expect(page.getByLabel("Title")).toBeDisabled();
});
