import { describe, expect, it } from "vitest";
import {
  angleInRange,
  angleOf,
  clampOffset,
  hitSegment,
  layoutSegments,
  polar,
  ringMetrics,
  sectorPath,
  stepIndex,
} from "./geometry";

describe("angleOf", () => {
  it("measures clockwise from 12 o'clock", () => {
    expect(angleOf(0, -10)).toBeCloseTo(0);
    expect(angleOf(10, 0)).toBeCloseTo(90);
    expect(angleOf(0, 10)).toBeCloseTo(180);
    expect(angleOf(-10, 0)).toBeCloseTo(270);
  });

  it("round-trips with polar", () => {
    for (const deg of [0, 33, 90, 181, 300]) {
      const p = polar(0, 0, 50, deg);
      expect(angleOf(p.x, p.y)).toBeCloseTo(deg, 6);
    }
  });
});

describe("layoutSegments", () => {
  it("centres the first segment at the top and leaves gaps", () => {
    const segs = layoutSegments(5, 4);
    expect(segs).toHaveLength(5);
    expect(segs[0]?.mid).toBe(0);
    expect(segs[0]?.start).toBeCloseTo(-34);
    expect(segs[0]?.end).toBeCloseTo(34);
    expect(segs[1]?.mid).toBe(72);
  });

  it("handles degenerate counts", () => {
    expect(layoutSegments(0)).toEqual([]);
    const [one] = layoutSegments(1);
    expect(one && one.end - one.start).toBe(360);
  });
});

describe("hitSegment", () => {
  it("ignores the pointer until it reaches the arm radius", () => {
    expect(hitSegment(0, -20, 5, 50)).toBeNull();
  });

  it("maps directions to segments, including across 0°", () => {
    expect(hitSegment(0, -100, 5, 50)).toBe(0);
    expect(hitSegment(-5, -100, 5, 50)).toBe(0); // just left of 12 o'clock
    const p = polar(0, 0, 100, 72);
    expect(hitSegment(p.x, p.y, 5, 50)).toBe(1);
    const q = polar(0, 0, 100, 290);
    expect(hitSegment(q.x, q.y, 5, 50)).toBe(4);
  });

  it("returns null for an empty ring", () => {
    expect(hitSegment(0, -100, 0, 50)).toBeNull();
  });
});

describe("angleInRange", () => {
  it("handles wrap-around", () => {
    expect(angleInRange(350, -20, 20)).toBe(true);
    expect(angleInRange(10, -20, 20)).toBe(true);
    expect(angleInRange(30, -20, 20)).toBe(false);
  });
});

describe("sectorPath", () => {
  it("produces a closed path with two arcs", () => {
    const d = sectorPath(100, 100, 50, 90, -30, 30);
    expect(d.startsWith("M")).toBe(true);
    expect(d.match(/A /g)).toHaveLength(2);
    expect(d.trim().endsWith("Z")).toBe(true);
  });

  it("draws a full ring for a single segment", () => {
    expect(sectorPath(100, 100, 50, 90, -180, 180).match(/A /g)).toHaveLength(4);
  });
});

describe("helpers", () => {
  it("clamps drag offsets to a radius", () => {
    const c = clampOffset(300, 400, 50);
    expect(Math.hypot(c.x, c.y)).toBeCloseTo(50);
    expect(clampOffset(3, 4, 50)).toEqual({ x: 3, y: 4 });
  });

  it("steps around the ring with wrap-around", () => {
    expect(stepIndex(0, -1, 5)).toBe(4);
    expect(stepIndex(4, 1, 5)).toBe(0);
    expect(stepIndex(2, 7, 5)).toBe(4);
  });

  it("keeps ring proportions sane at small sizes", () => {
    const m = ringMetrics(100);
    expect(m.size).toBe(260);
    expect(m.orb).toBeLessThan(m.inner);
    expect(m.inner).toBeLessThan(m.outer);
  });
});
