import { expect, test } from "@playwright/test";
import { chooseFormat, openSample, shot } from "./helpers";

test.describe("preview and editing", () => {
  test("rotate, adjust and crop update the preview and the summary", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "jpeg");
    await page.getByRole("button", { name: "Open preview" }).click();
    await expect(page.getByRole("img", { name: /Edited original preview/ })).toBeVisible();
    await expect(page.locator(".preview__caption")).toContainText("2400 × 1600");

    await page.locator(".preview").getByRole("button", { name: "Rotate right" }).click();
    await expect(page.locator(".preview__caption")).toContainText("1600 × 2400");

    await page.getByLabel("Brightness").fill("30");
    await expect(page.locator(".edit-summary")).toContainText("Rotated 90°");
    await expect(page.locator(".edit-summary")).toContainText("Tone adjusted");
    await shot(page, "15-edit");

    await page.locator(".preview").getByRole("button", { name: "Crop" }).click();
    const crop = page.getByRole("group", { name: /Crop area/ });
    await expect(crop).toBeVisible();
    await crop.focus();
    await page.keyboard.press("Shift+ArrowLeft");
    await page.keyboard.press("Shift+ArrowLeft");
    await shot(page, "16-crop");
    await page.getByRole("button", { name: "Apply crop" }).click();
    await expect(page.locator(".edit-summary")).toContainText("Cropped to");

    await page.locator(".edit-summary").getByRole("button", { name: "Reset" }).click();
    await expect(page.locator(".edit-summary")).toHaveCount(0);
    await page.getByRole("button", { name: "Formats" }).click();
    await expect(page.getByRole("radiogroup", { name: "Output format" })).toBeVisible();
  });

  test("resize keeps the aspect ratio unless unlocked", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "webp");
    await page.getByRole("switch", { name: /Resize/ }).click();
    const w = page.getByLabel("Width in pixels");
    const h = page.getByLabel("Height in pixels");
    await w.fill("1200");
    await expect(h).toHaveValue("800");
    await page.getByRole("button", { name: "Aspect ratio locked" }).click();
    await h.fill("500");
    await expect(w).toHaveValue("1200");
  });
});
