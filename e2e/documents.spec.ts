import { expect, test } from "@playwright/test";
import { chooseFormat, chooseOperation, openDocx, segment, setStageDelay, shot } from "./helpers";

test.describe("Word documents", () => {
  test.beforeEach(async ({ page }) => {
    await openDocx(page);
    await setStageDelay(page, 30);
  });

  test("only text export and properties are offered", async ({ page }) => {
    await expect(page.getByRole("region", { name: "Client memo.docx" }).getByText("Word document")).toBeVisible();
    for (const id of ["convert", "metadata"]) {
      await expect(segment(page, id)).not.toHaveAttribute("aria-disabled", "true");
    }
    for (const id of ["compress", "edit", "crop", "annotate", "redact", "background"]) {
      await expect(segment(page, id)).toHaveAttribute("aria-disabled", "true");
    }
    await shot(page, "50-docx-actions");
    await segment(page, "convert").click();
    await expect(page.getByRole("radiogroup", { name: "Output format" }).getByRole("radio")).toHaveCount(1);
  });

  test("extract text", async ({ page }) => {
    await chooseFormat(page, "txt");
    await page.getByRole("button", { name: /^Convert to Text/ }).click();
    await expect(page.getByRole("heading", { name: "Text extracted" })).toBeVisible();
  });

  test("edit properties and save a copy", async ({ page }) => {
    await chooseOperation(page, "metadata");
    await expect(page.getByText("Jane Doe")).toBeVisible();
    await page.getByRole("textbox", { name: "Author" }).fill("Account team");
    await shot(page, "51-docx-properties");
    await page.getByRole("button", { name: /Save metadata/ }).click();
    await expect(page.getByRole("heading", { name: "Document saved" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Open document" })).toBeVisible();
  });
});
