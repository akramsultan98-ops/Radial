import { describe, expect, it } from "vitest";
import { applyCropDrag } from "./PreviewView";
import { stemProblem } from "./OutputPanel";
import { editedSize } from "./OptionsPanel";
import { sampleFile } from "../lib/mockBackend";

const full = { x: 0, y: 0, width: 1, height: 1 };

describe("crop dragging", () => {
  it("moves without leaving the image", () => {
    const c = applyCropDrag({ x: 0.1, y: 0.1, width: 0.5, height: 0.5 }, "move", 0.9, -0.5);
    expect(c).toEqual({ x: 0.5, y: 0, width: 0.5, height: 0.5 });
  });

  it("resizes from each edge and enforces a minimum size", () => {
    expect(applyCropDrag(full, "se", -0.25, -0.5)).toEqual({ x: 0, y: 0, width: 0.75, height: 0.5 });
    const nw = applyCropDrag(full, "nw", 0.2, 0.3);
    expect(nw.x).toBeCloseTo(0.2);
    expect(nw.width).toBeCloseTo(0.8);
    expect(nw.height).toBeCloseTo(0.7);
    const tiny = applyCropDrag(full, "w", 5, 0);
    expect(tiny.width).toBeCloseTo(0.02);
    expect(tiny.x + tiny.width).toBeCloseTo(1);
  });
});

describe("file name validation", () => {
  it("mirrors the core rules", () => {
    expect(stemProblem("holiday")).toBeNull();
    expect(stemProblem("写真 🌅")).toBeNull();
    expect(stemProblem("")).toMatch(/Enter/);
    expect(stemProblem("a/b")).toMatch(/“\/”/);
    expect(stemProblem("a:b")).toMatch(/“:”/);
    expect(stemProblem("CON")).toMatch(/reserved/);
    expect(stemProblem("lpt1.txt")).toMatch(/reserved/);
    expect(stemProblem("dot.")).toMatch(/dot/);
    expect(stemProblem("tab\there")).toMatch(/control/);
  });
});

describe("edited size", () => {
  it("accounts for rotation and crop", () => {
    const f = sampleFile("x.png", { width: 2400, height: 1600 });
    expect(editedSize(f, { rotate: 90, crop: null, brightness: 0, contrast: 0 })).toEqual({ width: 1600, height: 2400 });
    expect(
      editedSize(f, { rotate: 0, crop: { x: 0, y: 0, width: 0.5, height: 0.25 }, brightness: 0, contrast: 0 }),
    ).toEqual({ width: 1200, height: 400 });
  });
});
