import { expect, test } from "@playwright/test";
import { chooseFormat, failNext, openSample, ready, setStageDelay, shot } from "./helpers";

test.describe("errors and recovery", () => {
  test("an existing output name is flagged before converting and fixed in one click", async ({ page }) => {
    await openSample(page);
    await page.evaluate(() => window.__radialMock?.existing.add("/home/demo/Pictures/taken.jpg"));
    await chooseFormat(page, "jpeg");
    const name = page.getByLabel("File name");
    await name.fill("taken");
    await expect(page.getByText("A file with this name exists.")).toBeVisible();
    await shot(page, "10-name-taken");
    await page.getByRole("button", { name: "Use “taken (2)”" }).click();
    await expect(name).toHaveValue("taken (2)");
  });

  test("a collision at write time offers a free name and succeeds", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "jpeg");
    // The name becomes taken after the suggestion was made.
    await page.evaluate(() => window.__radialMock?.existing.add("/home/demo/Pictures/Golden hour — 夕焼け.jpg"));
    await setStageDelay(page, 50);
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "That name is taken" })).toBeVisible();
    await shot(page, "11-error-exists");
    await page.getByRole("button", { name: "Use a free name" }).click();
    await expect(page.getByRole("heading", { name: "Converted to JPEG" })).toBeVisible();
    await expect(page.getByText("Golden hour — 夕焼け (2).jpg").first()).toBeVisible();
  });

  test("an engine failure can be retried", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "webp");
    await setStageDelay(page, 50);
    await failNext(page, "engine_failed", "The output could not be read back.");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "The conversion failed" })).toBeVisible();
    await expect(page.getByRole("alert")).toContainText("could not be read back");
    await page.getByRole("button", { name: "Try again" }).click();
    await expect(page.getByRole("heading", { name: "Converted to WebP" })).toBeVisible();
  });

  test("an unwritable destination offers another folder", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "tiff");
    await setStageDelay(page, 30);
    await failNext(page, "destination_not_writable", "RADIAL cannot write to that folder.");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Can't save there" })).toBeVisible();
    await page.getByRole("button", { name: "Choose another folder" }).click();
    await expect(page.getByText("/home/demo/Exports")).toBeVisible();
    await expect(page.getByRole("button", { name: /Convert to TIFF/ })).toBeEnabled();
  });

  test("cancelling stops the job and nothing is written", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "jpeg");
    await setStageDelay(page, 600);
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Converting…" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("heading", { name: "Conversion cancelled" })).toBeVisible({ timeout: 5000 });
    await expect(page.getByText("Nothing was written to disk.")).toBeVisible();
    await shot(page, "12-cancelled");
    await page.getByRole("button", { name: "Back to options" }).click();
    await expect(page.getByRole("button", { name: /Convert to JPEG/ })).toBeVisible();
  });

  test("an unreadable file is rejected with a clear reason", async ({ page }) => {
    await ready(page);
    await page.evaluate(() => {
      const m = window.__radialMock;
      if (!m) return;
      m.nextPick = [
        {
          type: "file",
          ...m.sampleFile("report.pdf", {
            format: null,
            recognized: { kind: "known", format: { id: "pdf", label: "PDF", category: "document" } },
            width: null,
            height: null,
            hasAlpha: false,
            bitDepth: null,
            problem: {
              kind: "unsupported_format",
              message: "PDF files are recognised but cannot be converted yet.",
              retryable: false,
            },
          }),
        },
      ];
    });
    await page.getByRole("button", { name: "Choose file" }).click();
    await expect(page.getByRole("heading", { name: "This file can't be converted" })).toBeVisible();
    await expect(page.getByText("PDF files are recognised but cannot be converted yet.").first()).toBeVisible();
    await expect(page.getByRole("radiogroup", { name: "Output format" })).toHaveCount(0);
    await shot(page, "13-rejected");
  });

  test("dropping several files explains how they are handled", async ({ page }) => {
    await ready(page);
    await page.evaluate(() => {
      const m = window.__radialMock;
      if (!m) return;
      m.drop([
        { type: "error", name: "folder", error: { kind: "invalid_file", message: "That is a folder, not a file.", retryable: false } },
        { type: "file", ...m.sampleFile("one.png") },
        { type: "file", ...m.sampleFile("two.png") },
      ]);
    });
    await expect(page.getByText(/3 files added · 1 can't be converted/)).toBeVisible();
    await expect(page.getByRole("heading", { name: "one.png" })).toBeVisible();
    await shot(page, "14-multi-drop");
    await page.getByRole("listbox", { name: "Imported files" }).getByRole("button", { name: "two.png" }).click();
    await expect(page.getByRole("heading", { name: "two.png" })).toBeVisible();
  });
});
