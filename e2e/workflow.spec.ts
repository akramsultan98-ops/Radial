import { expect, test, type Locator } from "@playwright/test";
import { chooseFormat, openSample, ready, segment, shot } from "./helpers";

async function center(l: Locator) {
  const b = await l.boundingBox();
  if (!b) throw new Error("element not visible");
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

test.describe("core workflow", () => {
  test("empty state invites a file and lists registry formats", async ({ page }) => {
    await ready(page);
    await expect(page.getByRole("heading", { name: "Drop a file to begin" })).toBeVisible();
    const formats = page.getByRole("list", { name: "Formats RADIAL can open" });
    await expect(formats.getByRole("listitem")).toHaveText(["PNG", "JPEG", "WebP", "BMP", "TIFF", "GIF", "PDF", "Text", "Word"]);
    await expect(page.getByText("Local processing — files never leave this computer")).toBeVisible();
    await shot(page, "01-empty");
  });

  test("drag-over state is shown while files hover the window", async ({ page }) => {
    await ready(page);
    await page.evaluate(() => window.__radialMock?.drag("over"));
    await expect(page.getByRole("heading", { name: "Release to open" })).toBeVisible();
    await shot(page, "02-drag-over");
    await page.evaluate(() => window.__radialMock?.drag("leave"));
    await expect(page.getByRole("heading", { name: "Drop a file to begin" })).toBeVisible();
  });

  test("importing a file shows metadata and the actions ring", async ({ page }) => {
    await openSample(page);
    await expect(page.getByRole("heading", { name: "Golden hour — 夕焼け.png" })).toBeVisible();
    await expect(page.getByText("2400 × 1600").first()).toBeVisible();
    const ring = page.getByRole("radiogroup", { name: "Actions", exact: true });
    await expect(ring.getByRole("radio")).toHaveCount(8);
    await expect(ring.getByRole("radio").first()).toHaveAttribute("data-id", "convert");
    // The accessible list offers the same actions.
    await expect(page.getByRole("radiogroup", { name: "Actions (list)" }).getByRole("radio")).toHaveCount(8);
    await shot(page, "03-actions");
  });

  test("actions depend on the file: BMP can't be compressed and says why", async ({ page }) => {
    await ready(page);
    await page.evaluate(() => {
      const m = window.__radialMock!;
      m.nextPick = [{ type: "file", ...m.sampleFile("scan.bmp", { format: "bmp", hasAlpha: false }) }];
    });
    await page.getByRole("button", { name: "Choose file" }).click();
    const compress = segment(page, "compress");
    await expect(compress).toHaveAttribute("aria-disabled", "true");
    await expect(compress).toHaveAttribute("aria-label", /Convert to PNG for a lossless smaller file/);
    await compress.click({ force: true });
    await expect(page.getByRole("heading", { level: 2, name: "Compress", exact: true })).toHaveCount(0);
  });

  test("hovering a segment highlights it", async ({ page }) => {
    await openSample(page);
    await segment(page, "compress").hover();
    await expect(segment(page, "compress")).toHaveClass(/seg--hover/);
    await shot(page, "04-hover");
  });

  test("convert opens a ring of formats; options follow the chosen format", async ({ page }) => {
    await openSample(page);
    await segment(page, "convert").click();
    const ring = page.getByRole("radiogroup", { name: "Output format" });
    await expect(ring.getByRole("radio")).toHaveCount(6);
    await expect(segment(page, "png")).toHaveCount(0);
    await chooseFormat(page, "jpeg");
    await expect(segment(page, "jpeg")).toHaveAttribute("aria-checked", "true");
    await expect(page.getByLabel(/Quality/)).toHaveValue("90");
    await expect(page.getByLabel(/Background/)).toBeVisible();
    await shot(page, "05-convert-jpeg");
    await segment(page, "webp").click();
    await expect(page.getByRole("switch", { name: /Lossless/ })).toBeVisible();
    await page.getByRole("button", { name: "All actions" }).click();
    await expect(page.getByRole("radiogroup", { name: "Actions", exact: true })).toBeVisible();
  });

  test("dragging the file onto an action selects it", async ({ page }) => {
    await openSample(page);
    const from = await center(page.locator(".radial__orb"));
    const to = await center(segment(page, "compress"));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 12 });
    await expect(segment(page, "compress")).toHaveClass(/seg--armed/);
    await expect(page.getByText(/Release:/)).toBeVisible();
    await shot(page, "06-drag-armed");
    await page.mouse.up();
    await expect(page.getByRole("heading", { level: 2, name: "Compress", exact: true })).toBeVisible();
  });

  test("an accidental nudge or a drag released inside the ring selects nothing", async ({ page }) => {
    await openSample(page);
    const c = await center(page.locator(".radial__orb"));
    await page.mouse.move(c.x, c.y);
    await page.mouse.down();
    await page.mouse.move(c.x + 3, c.y + 2);
    await page.mouse.up();
    await page.mouse.move(c.x, c.y);
    await page.mouse.down();
    await page.mouse.move(c.x + 40, c.y + 10, { steps: 5 });
    await page.mouse.up();
    await expect(page.getByRole("radiogroup", { name: "Actions", exact: true }).getByRole("radio", { checked: true })).toHaveCount(0);
  });

  test("keyboard: arrows move around the ring and Enter selects", async ({ page }) => {
    await openSample(page);
    await segment(page, "convert").focus();
    await page.keyboard.press("ArrowRight");
    await expect(segment(page, "compress")).toBeFocused();
    await shot(page, "07-keyboard-focus");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { level: 2, name: "Compress", exact: true })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("radiogroup", { name: "Actions", exact: true })).toBeVisible();
  });

  test("the list alternative selects actions and formats without the ring", async ({ page }) => {
    await openSample(page);
    await page.getByRole("radiogroup", { name: "Actions (list)" }).getByRole("radio", { name: /Convert/ }).click();
    await page.getByRole("radiogroup", { name: "Convert to (list)" }).getByRole("radio", { name: /BMP/ }).click();
    await expect(segment(page, "bmp")).toHaveAttribute("aria-checked", "true");
  });

  test("converting shows stages, then the result with actions and preview", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "jpeg");
    await page.getByRole("button", { name: /Convert to JPEG/ }).click();
    await expect(page.getByRole("heading", { name: "Converting…" })).toBeVisible();
    await expect(page.locator(".radial--busy")).toBeVisible();
    await expect(page.locator('.stages [aria-current="step"]')).toBeVisible();
    await shot(page, "08-converting");
    await expect(page.getByRole("heading", { name: "Converted to JPEG" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("Golden hour — 夕焼け.jpg").first()).toBeVisible();
    await expect(page.getByRole("button", { name: "Open", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Show in folder" })).toBeVisible();
    await expect(page.getByRole("img", { name: /Result · JPEG preview/ })).toBeVisible();
    await shot(page, "09-done");

    await page.getByRole("radio", { name: "Original" }).click();
    await expect(page.getByRole("img", { name: /Original · PNG preview/ })).toBeVisible();
    await page.getByRole("button", { name: "Actual size (100%)" }).click();
    await expect(page.locator(".preview__zoom")).toHaveText("100%");
    await page.getByRole("button", { name: "Fit to window" }).click();
    await page.getByRole("button", { name: "Zoom in" }).click();

    await page.getByRole("button", { name: "Another action" }).click();
    await expect(page.getByRole("radiogroup", { name: "Actions", exact: true })).toBeVisible();
  });

  test("Ctrl+Enter runs the chosen action", async ({ page }) => {
    await openSample(page);
    await chooseFormat(page, "webp");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("heading", { name: "Converted to WebP" })).toBeVisible({ timeout: 10_000 });
  });
});
