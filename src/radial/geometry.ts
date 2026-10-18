// Geometry for the radial menu. Angles are in degrees, measured clockwise
// from 12 o'clock (screen coordinates: +y points down).

export interface Point {
  x: number;
  y: number;
}

export interface Segment {
  index: number;
  /** Start and end angles, clockwise from 12 o'clock. */
  start: number;
  end: number;
  mid: number;
}

const rad = (deg: number) => (deg * Math.PI) / 180;

export function polar(cx: number, cy: number, r: number, deg: number): Point {
  return { x: cx + r * Math.sin(rad(deg)), y: cy - r * Math.cos(rad(deg)) };
}

/** Angle of a vector in [0, 360), clockwise from 12 o'clock. */
export function angleOf(dx: number, dy: number): number {
  const a = (Math.atan2(dx, -dy) * 180) / Math.PI;
  return (a + 360) % 360;
}

/** Split the circle into `n` segments centred on 12 o'clock, separated by `gap` degrees. */
export function layoutSegments(n: number, gap = 4): Segment[] {
  if (n <= 0) return [];
  const span = 360 / n;
  const g = n === 1 ? 0 : Math.min(gap, span / 3);
  return Array.from({ length: n }, (_, i) => {
    const mid = i * span;
    return { index: i, start: mid - span / 2 + g / 2, end: mid + span / 2 - g / 2, mid };
  });
}

/** SVG path for an annular sector. */
export function sectorPath(cx: number, cy: number, r0: number, r1: number, start: number, end: number): string {
  if (end - start >= 359.999) {
    // Full ring: two half-arcs per edge, drawn with even-odd fill.
    const a = polar(cx, cy, r1, 0);
    const b = polar(cx, cy, r1, 180);
    const c = polar(cx, cy, r0, 0);
    const d = polar(cx, cy, r0, 180);
    return [
      `M ${f(a.x)} ${f(a.y)} A ${f(r1)} ${f(r1)} 0 1 1 ${f(b.x)} ${f(b.y)} A ${f(r1)} ${f(r1)} 0 1 1 ${f(a.x)} ${f(a.y)} Z`,
      `M ${f(c.x)} ${f(c.y)} A ${f(r0)} ${f(r0)} 0 1 0 ${f(d.x)} ${f(d.y)} A ${f(r0)} ${f(r0)} 0 1 0 ${f(c.x)} ${f(c.y)} Z`,
    ].join(" ");
  }
  const large = end - start > 180 ? 1 : 0;
  const p1 = polar(cx, cy, r1, start);
  const p2 = polar(cx, cy, r1, end);
  const p3 = polar(cx, cy, r0, end);
  const p4 = polar(cx, cy, r0, start);
  return [
    `M ${f(p1.x)} ${f(p1.y)}`,
    `A ${f(r1)} ${f(r1)} 0 ${large} 1 ${f(p2.x)} ${f(p2.y)}`,
    `L ${f(p3.x)} ${f(p3.y)}`,
    `A ${f(r0)} ${f(r0)} 0 ${large} 0 ${f(p4.x)} ${f(p4.y)}`,
    "Z",
  ].join(" ");
}

const f = (v: number) => (Math.round(v * 100) / 100).toString();

/** Whether `angle` lies within [start, end], handling wrap-around at 0°. */
export function angleInRange(angle: number, start: number, end: number): boolean {
  const norm = (a: number) => ((a % 360) + 360) % 360;
  const a = norm(angle);
  const s = norm(start);
  const e = norm(end);
  return s <= e ? a >= s && a <= e : a >= s || a <= e;
}

/**
 * Which segment a pointer offset (from the centre) targets while dragging.
 * Gaps count toward the nearer segment so the ring has no dead zones, and the
 * pointer only needs to reach `armRadius` (it need not stay inside the ring).
 */
export function hitSegment(dx: number, dy: number, count: number, armRadius: number): number | null {
  if (count <= 0) return null;
  if (Math.hypot(dx, dy) < armRadius) return null;
  const span = 360 / count;
  const a = angleOf(dx, dy);
  return Math.round(a / span) % count;
}

/** Clamp a drag offset so the dragged thumbnail stays within `max` of the centre. */
export function clampOffset(dx: number, dy: number, max: number): Point {
  const d = Math.hypot(dx, dy);
  if (d <= max || d === 0) return { x: dx, y: dy };
  return { x: (dx / d) * max, y: (dy / d) * max };
}

/** Ring dimensions for a square stage of side `size`. */
export function ringMetrics(size: number) {
  const s = Math.max(260, size);
  const outer = s / 2 - 8;
  const inner = outer * 0.62;
  const orb = inner - Math.max(14, s * 0.035);
  return { size: s, center: s / 2, inner, outer, orb, labelRadius: (inner + outer) / 2 };
}

/** Index reached by moving `delta` steps around a ring of `count`. */
export function stepIndex(current: number, delta: number, count: number): number {
  if (count <= 0) return -1;
  return (((current + delta) % count) + count) % count;
}
