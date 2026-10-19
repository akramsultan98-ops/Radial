// Interactive layers drawn over the preview: annotations and redaction areas.
// Geometry is stored as fractions of the image so it matches what the native
// engine draws into the exported file (see crates/radial-core/src/draw.rs).

import { useRef, useState, type PointerEvent } from "react";
import type { CropRect, Shape } from "../lib/types";
import type { AnnotateTool } from "./OperationPanels";

interface Box {
  /** Displayed size in CSS pixels. */
  w: number;
  h: number;
  /** Image size in pixels (for stroke widths). */
  natural: { width: number; height: number };
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

function usePointerDrag(box: Box) {
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const [current, setCurrent] = useState<{ x1: number; y1: number; x2: number; y2: number } | null>(null);
  const frac = (e: PointerEvent<Element>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: clamp01((e.clientX - r.left) / box.w), y: clamp01((e.clientY - r.top) / box.h) };
  };
  return { startRef, current, setCurrent, frac };
}

/** SVG for one shape in image-pixel coordinates. */
export function ShapeSvg({ s, natural }: { s: Shape; natural: { width: number; height: number } }) {
  const W = natural.width;
  const H = natural.height;
  const short = Math.min(W, H);
  const sw = Math.max(1, s.stroke * short);
  const [ax, ay, bx, by] = [s.x1 * W, s.y1 * H, s.x2 * W, s.y2 * H];
  const common = { stroke: s.color, strokeWidth: sw, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, fill: "none" };
  switch (s.kind) {
    case "line":
      return <line x1={ax} y1={ay} x2={bx} y2={by} {...common} />;
    case "arrow": {
      const len = Math.hypot(bx - ax, by - ay);
      if (len < 1) return null;
      const ux = (bx - ax) / len;
      const uy = (by - ay) / len;
      const head = Math.min(Math.max(sw * 4, short * 0.02), len * 0.6);
      const base = [bx - ux * head, by - uy * head] as const;
      const half = head * 0.55;
      return (
        <g>
          <line x1={ax} y1={ay} x2={base[0]} y2={base[1]} {...common} />
          <polygon
            points={`${bx},${by} ${base[0] - uy * half},${base[1] + ux * half} ${base[0] + uy * half},${base[1] - ux * half}`}
            fill={s.color}
          />
        </g>
      );
    }
    case "rect":
      return (
        <rect
          x={Math.min(ax, bx)}
          y={Math.min(ay, by)}
          width={Math.abs(bx - ax)}
          height={Math.abs(by - ay)}
          {...common}
          strokeLinecap="butt"
          fill={s.filled ? s.color : "none"}
          stroke={s.filled ? "none" : s.color}
        />
      );
    case "ellipse":
      return (
        <ellipse
          cx={(ax + bx) / 2}
          cy={(ay + by) / 2}
          rx={Math.abs(bx - ax) / 2}
          ry={Math.abs(by - ay) / 2}
          {...common}
          fill={s.filled ? s.color : "none"}
          stroke={s.filled ? "none" : s.color}
        />
      );
    case "text": {
      const size = Math.max(6, s.size * short);
      return (
        <text x={ax} y={ay} fill={s.color} fontSize={size} fontFamily="'DejaVu Sans', 'Inter Variable', sans-serif" dominantBaseline="hanging">
          {s.text.split("\n").map((line, i) => (
            <tspan key={i} x={ax} dy={i === 0 ? 0 : size * 1.15}>
              {line}
            </tspan>
          ))}
        </text>
      );
    }
  }
}

export function AnnotationLayer({
  box,
  shapes,
  tool,
  page = 0,
  onAdd,
}: {
  box: Box;
  shapes: Shape[];
  tool: AnnotateTool;
  /** PDF page new shapes belong to. */
  page?: number;
  onAdd: (s: Shape) => void;
}) {
  const { startRef, current, setCurrent, frac } = usePointerDrag(box);
  const make = (x1: number, y1: number, x2: number, y2: number): Shape => ({
    kind: tool.kind,
    x1,
    y1,
    x2,
    y2,
    color: tool.color,
    stroke: tool.stroke,
    filled: tool.filled,
    text: tool.text,
    size: tool.size,
    page,
  });

  const down = (e: PointerEvent<SVGSVGElement>) => {
    if (e.button !== 0) return;
    const p = frac(e);
    if (tool.kind === "text") {
      if (tool.text.trim()) onAdd(make(p.x, p.y, p.x, p.y));
      return;
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    startRef.current = p;
    setCurrent({ x1: p.x, y1: p.y, x2: p.x, y2: p.y });
  };
  const move = (e: PointerEvent<SVGSVGElement>) => {
    if (!startRef.current) return;
    const p = frac(e);
    setCurrent({ x1: startRef.current.x, y1: startRef.current.y, x2: p.x, y2: p.y });
  };
  const up = () => {
    if (current && Math.hypot((current.x2 - current.x1) * box.w, (current.y2 - current.y1) * box.h) > 4) {
      onAdd(make(current.x1, current.y1, current.x2, current.y2));
    }
    startRef.current = null;
    setCurrent(null);
  };

  return (
    <svg
      className={`editor-layer editor-layer--${tool.kind === "text" ? "text" : "draw"}`}
      width={box.w}
      height={box.h}
      viewBox={`0 0 ${box.natural.width} ${box.natural.height}`}
      preserveAspectRatio="none"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={() => {
        startRef.current = null;
        setCurrent(null);
      }}
      role="img"
      aria-label={`Annotation canvas with ${shapes.length} annotation${shapes.length === 1 ? "" : "s"}`}
    >
      {shapes.map((s, i) => (
        <ShapeSvg key={i} s={s} natural={box.natural} />
      ))}
      {current && <ShapeSvg s={make(current.x1, current.y1, current.x2, current.y2)} natural={box.natural} />}
    </svg>
  );
}

export function RedactLayer({ box, regions, onAdd }: { box: Box; regions: CropRect[]; onAdd: (r: CropRect) => void }) {
  const { startRef, current, setCurrent, frac } = usePointerDrag(box);
  const toRect = (c: { x1: number; y1: number; x2: number; y2: number }): CropRect => ({
    x: Math.min(c.x1, c.x2),
    y: Math.min(c.y1, c.y2),
    width: Math.abs(c.x2 - c.x1),
    height: Math.abs(c.y2 - c.y1),
  });
  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = frac(e);
    startRef.current = p;
    setCurrent({ x1: p.x, y1: p.y, x2: p.x, y2: p.y });
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    if (!startRef.current) return;
    const p = frac(e);
    setCurrent({ x1: startRef.current.x, y1: startRef.current.y, x2: p.x, y2: p.y });
  };
  const up = () => {
    if (current) {
      const r = toRect(current);
      if (r.width * box.w > 3 && r.height * box.h > 3) onAdd(r);
    }
    startRef.current = null;
    setCurrent(null);
  };
  const style = (r: CropRect) => ({
    left: r.x * box.w,
    top: r.y * box.h,
    width: r.width * box.w,
    height: r.height * box.h,
  });
  return (
    <div
      className="editor-layer editor-layer--draw"
      style={{ width: box.w, height: box.h }}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={() => {
        startRef.current = null;
        setCurrent(null);
      }}
      role="img"
      aria-label={`Redaction canvas with ${regions.length} area${regions.length === 1 ? "" : "s"}`}
    >
      {regions.map((r, i) => (
        <div key={i} className="redact-box" style={style(r)}>
          <span>{i + 1}</span>
        </div>
      ))}
      {current && <div className="redact-box redact-box--draft" style={style(toRect(current))} />}
    </div>
  );
}
