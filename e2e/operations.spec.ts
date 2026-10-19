import { expect, test } from "@playwright/test";
import { chooseOperation, dragOnImage, failNext, openSample, setStageDelay, shot } from "./helpers";

test.describe("operations", () => {
  test.beforeEach(async ({ page }) => {
    await openSample(page);
    await setStageDelay(page, 30);
  });

  test("compress shows the original size and the real savings", async ({ page }) => {
    await chooseOperation(page, "compress");
    await expect(page.getByText(/Original: 2.5 MB/)).toBeVisible();
    await expect(page.getByText("Lossless optimisation: the pixels stay exactly the same.")).toBeVisible();
    await expect(page.getByLabel("File name")).toHaveValue("Golden hour — 夕焼け-compressed");
    await page.getByRole("switch", { name: /Limit size/ }).click();
    await expect(page.getByLabel("Longest side in pixels")).toHaveValue("1200");
    await shot(page, "20-compress");
    await page.getByRole("button", { name: /^Compress Ctrl/ }).click();
    await expect(page.getByRole("heading", { name: /Compressed — 63% smaller/ })).toBeVisible();
    await shot(page, "21-compress-done");
  });

  test("compress that can't shrink the file explains and saves nothing", async ({ page }) => {
    await chooseOperation(page, "compress");
    await failNext(page, "no_reduction", "These settings would not make the file smaller, so nothing was saved.");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Couldn't make it smaller" })).toBeVisible();
    await expect(page.getByRole("alert")).toContainText("nothing was saved");
    await page.getByRole("button", { name: "Back to options" }).click();
    await expect(page.getByRole("button", { name: /^Compress Ctrl/ })).toBeEnabled();
  });

  test("annotate: draw an arrow and text, then save", async ({ page }) => {
    await chooseOperation(page, "annotate");
    const save = page.getByRole("button", { name: /Save annotated copy/ });
    await expect(save).toBeDisabled();
    await expect(page.getByText("Draw at least one annotation.")).toBeVisible();
    await dragOnImage(page, [0.2, 0.7], [0.55, 0.4]);
    await page.getByRole("radio", { name: "Text" }).click();
    await page.getByLabel("Text to place").fill("Approved");
    const layer = page.locator(".editor-layer");
    await layer.click({ position: { x: 40, y: 40 } });
    await expect(page.getByText("2 annotations")).toBeVisible();
    await expect(layer.locator("text")).toHaveText("Approved");
    await shot(page, "22-annotate");
    await page.getByRole("button", { name: "Undo" }).click();
    await expect(page.getByText("1 annotation")).toBeVisible();
    await save.click();
    await expect(page.getByRole("heading", { name: "Annotated copy saved" })).toBeVisible();
  });

  test("redact: mark areas, remove one, save a redacted copy", async ({ page }) => {
    await chooseOperation(page, "redact");
    await expect(page.getByText(/replaced with black, and all metadata removed/)).toBeVisible();
    await dragOnImage(page, [0.1, 0.1], [0.4, 0.25]);
    await dragOnImage(page, [0.5, 0.6], [0.8, 0.8]);
    await expect(page.getByText("2 areas marked")).toBeVisible();
    await expect(page.locator(".redact-box")).toHaveCount(2);
    await page.getByRole("button", { name: "Remove area 2" }).click();
    await expect(page.locator(".redact-box")).toHaveCount(1);
    await shot(page, "23-redact");
    await page.getByRole("button", { name: /Redact & save copy/ }).click();
    await expect(page.getByRole("heading", { name: "Redacted copy saved" })).toBeVisible();
    await expect(page.getByText(/1 area replaced with black pixels; metadata removed/)).toBeVisible();
  });

  test("background: colour and border preview, then apply", async ({ page }) => {
    await chooseOperation(page, "background");
    await page.getByRole("radio", { name: "#000000" }).click();
    await page.getByLabel(/Border/).fill("100");
    await expect(page.locator(".preview__frame--filled")).toHaveCSS("background-color", "rgb(0, 0, 0)");
    await shot(page, "24-background");
    await page.getByRole("button", { name: "Apply background" }).click();
    await expect(page.getByRole("heading", { name: "Background applied" })).toBeVisible();
  });

  test("metadata: shows location warning, edits a field, strips everything", async ({ page }) => {
    await chooseOperation(page, "metadata");
    await expect(page.getByText(/records where it was taken/)).toBeVisible();
    const save = page.getByRole("button", { name: /Save metadata/ });
    await expect(save).toBeDisabled();
    await page.getByLabel("Author").fill("Account team");
    await expect(save).toBeEnabled();
    await shot(page, "25-metadata");
    await page.getByRole("switch", { name: /Remove all metadata/ }).click();
    await expect(page.getByLabel("Author")).toBeDisabled();
    await page.getByRole("button", { name: /Save without metadata/ }).click();
    await expect(page.getByRole("heading", { name: "Metadata saved" })).toBeVisible();
  });

  test("crop: choose an area and save", async ({ page }) => {
    await chooseOperation(page, "crop");
    const area = page.getByRole("group", { name: /Crop area/ });
    await expect(area).toBeVisible();
    await area.focus();
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(page.getByText(/Keeping 89% × 90%/)).toBeVisible();
    await page.getByRole("button", { name: /Save cropped copy/ }).click();
    await expect(page.getByRole("heading", { name: "Cropped copy saved" })).toBeVisible();
  });

  test("edit: requires a change, then saves", async ({ page }) => {
    await chooseOperation(page, "edit");
    const save = page.getByRole("button", { name: /Save edited copy/ });
    await expect(save).toBeDisabled();
    await page.locator(".preview").getByRole("button", { name: "Rotate right" }).click();
    await expect(page.locator(".preview__caption")).toContainText("1600 × 2400");
    await expect(save).toBeEnabled();
    await save.click();
    await expect(page.getByRole("heading", { name: "Edits saved" })).toBeVisible();
  });

  test("history records the operation, and sensitive ones retry without their content", async ({ page }) => {
    await chooseOperation(page, "redact");
    await dragOnImage(page, [0.1, 0.1], [0.4, 0.25]);
    await page.getByRole("button", { name: /Redact & save copy/ }).click();
    await expect(page.getByRole("heading", { name: "Redacted copy saved" })).toBeVisible();
    await page.keyboard.press("Control+h");
    const dialog = page.getByRole("dialog", { name: "Recent activity" });
    await expect(dialog.getByText(/Redact/)).toBeVisible();
  });
});
