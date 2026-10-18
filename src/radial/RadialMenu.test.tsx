import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Capability, Format } from "../lib/types";
import { RadialMenu } from "./RadialMenu";

const cap = (to: Format, available = true): Capability => ({
  from: "png",
  to,
  toLabel: to.toUpperCase(),
  engine: "native_image",
  engineLabel: "Built-in",
  engineStatus: available ? { state: "available" } : { state: "missing", hint: "Install FFmpeg" },
  available,
  options: [],
  limitations: [],
  progress: "stages",
  cancellation: "between_stages",
});

const caps = [cap("jpeg"), cap("webp"), cap("bmp", false), cap("tiff")];

function setup(selected: Format | null = null, busy = false) {
  const onSelect = vi.fn();
  render(
    <RadialMenu capabilities={caps} selected={selected} onSelect={onSelect} size={480} busy={busy} sourceLabel="PNG">
      <span>orb</span>
    </RadialMenu>,
  );
  return { onSelect, radios: screen.getAllByRole("radio") };
}

describe("RadialMenu", () => {
  it("renders one segment per capability, in registry order", () => {
    const { radios } = setup();
    expect(radios.map((r) => r.getAttribute("data-format"))).toEqual(["jpeg", "webp", "bmp", "tiff"]);
    expect(screen.getByRole("radiogroup", { name: "Output format" })).toBeInTheDocument();
  });

  it("selects on click", () => {
    const { onSelect, radios } = setup();
    fireEvent.click(radios[1]!);
    expect(onSelect).toHaveBeenCalledWith("webp");
  });

  it("never selects an unavailable format and explains why", () => {
    const { onSelect, radios } = setup();
    const bmp = radios[2]!;
    expect(bmp).toHaveAttribute("aria-disabled", "true");
    expect(bmp.getAttribute("aria-label")).toMatch(/Install FFmpeg/);
    fireEvent.click(bmp);
    fireEvent.keyDown(bmp, { key: "Enter" });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("supports roving focus with arrow keys and selection with Enter/Space", () => {
    const { onSelect, radios } = setup();
    expect(radios[0]).toHaveAttribute("tabindex", "0");
    expect(radios[1]).toHaveAttribute("tabindex", "-1");
    radios[0]!.focus();
    fireEvent.keyDown(radios[0]!, { key: "ArrowRight" });
    expect(document.activeElement).toBe(radios[1]);
    fireEvent.keyDown(radios[1]!, { key: "ArrowLeft" });
    fireEvent.keyDown(radios[0]!, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(radios[3]); // wraps around
    fireEvent.keyDown(radios[3]!, { key: "Enter" });
    expect(onSelect).toHaveBeenLastCalledWith("tiff");
    fireEvent.keyDown(radios[3]!, { key: "Home" });
    fireEvent.keyDown(radios[0]!, { key: " " });
    expect(onSelect).toHaveBeenLastCalledWith("jpeg");
  });

  it("supports number shortcuts", () => {
    const { onSelect, radios } = setup();
    fireEvent.keyDown(radios[0]!, { key: "2" });
    expect(onSelect).toHaveBeenCalledWith("webp");
  });

  it("marks the selection and makes it the tab stop", () => {
    const { radios } = setup("tiff");
    expect(radios[3]).toHaveAttribute("aria-checked", "true");
    expect(radios[3]).toHaveAttribute("tabindex", "0");
    expect(radios[0]).toHaveAttribute("aria-checked", "false");
  });

  it("ignores input while busy", () => {
    const { onSelect, radios } = setup("jpeg", true);
    fireEvent.click(radios[1]!);
    expect(onSelect).not.toHaveBeenCalled();
  });
});
