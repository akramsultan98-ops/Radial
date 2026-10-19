import { expect, test } from "@playwright/test";
import { chooseFormat, failNext, openSample, ready, setStageDelay, shot } from "./helpers";

test.describe("history, settings and layout", () => {
  test("history records conversions and can be cleared", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "jpeg");
    await setStageDelay(page, 30);
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Converted to JPEG" })).toBeVisible();

    await page.getByRole("button", { name: /Recent activity/ }).click();
    const dialog = page.getByRole("dialog", { name: "Recent activity" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/PNG → JPEG/)).toBeVisible();
    await expect(dialog.getByRole("button", { name: /^Open / })).toBeVisible();
    await shot(page, "17-history");
    await dialog.getByRole("button", { name: "Clear history" }).click();
    await dialog.getByRole("button", { name: "Clear history" }).click();
    await expect(dialog.getByText("Nothing here yet.")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
  });

  test("failed conversions can be retried from history", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "gif");
    await setStageDelay(page, 30);
    await failNext(page, "io", "Disk hiccup.");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Something went wrong" })).toBeVisible();
    await page.keyboard.press("Control+h");
    await page.getByRole("button", { name: /^Retry / }).click();
    await expect(page.getByRole("button", { name: /Convert to GIF/ })).toBeVisible();
  });

  test("settings change conversion defaults", async ({ page }) => {
    await ready(page);
    await page.getByRole("button", { name: "Settings" }).click();
    const dialog = page.getByRole("dialog", { name: "Settings" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel(/JPEG quality/).fill("70");
    await dialog.getByRole("radio", { name: "Reduced" }).click();
    await shot(page, "18-settings");
    await dialog.getByRole("button", { name: "Save" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(page.locator("html")).toHaveAttribute("data-motion", "reduced");
    await page.getByRole("button", { name: "Choose file" }).click();
    await chooseFormat(page, "jpeg");
    await expect(page.getByLabel(/Quality/)).toHaveValue("70");
  });

  test("dialogs move focus in and restore it on close", async ({ page }) => {
    await ready(page);
    const trigger = page.getByRole("button", { name: "Settings" });
    await trigger.click();
    await expect(page.getByRole("dialog", { name: "Settings" }).getByRole("button", { name: "Close" })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(trigger).toBeFocused();
  });

  for (const size of [
    { width: 880, height: 620, name: "minimum" },
    { width: 1600, height: 1000, name: "large" },
  ]) {
    test(`layout fits a ${size.name} window without clipping`, async ({ page }) => {
      await page.setViewportSize({ width: size.width, height: size.height });
      await openSample(page);
      const stage = await page.locator(".stage").boundingBox();
      const ring = await page.locator(".radial").boundingBox();
      if (!stage || !ring) throw new Error("not visible");
      expect(ring.x).toBeGreaterThanOrEqual(stage.x);
      expect(ring.y).toBeGreaterThanOrEqual(stage.y);
      expect(ring.x + ring.width).toBeLessThanOrEqual(stage.x + stage.width);
      expect(ring.y + ring.height).toBeLessThanOrEqual(stage.y + stage.height);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
      expect(overflow).toBe(false);
      await shot(page, `19-layout-${size.name}`);
    });
  }
});
