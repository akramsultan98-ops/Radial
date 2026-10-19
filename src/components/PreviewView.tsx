// Zoomable preview of a source or output image, with an optional crop
// overlay. Pixels come from the native preview pipeline, so what you see is
// what the exporter will produce (minus resize).

import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode, type WheelEvent } from "react";
import type { Backend } from "../lib/backend";
import type { CropRect, Edits } from "../lib/types";
import { usePreview } from "../state/usePreview";
import { IconActualSize, IconFit, IconZoomIn, IconZoomOut } from "./Icons";

export interface CropControl {
  value: CropRect;
  onChange: (c: CropRect) => void;
}

interface Props {
  backend: Backend | null;
  path: string;
  edits: Edits;
  /** Full-resolution size of the rendered image (after rotate/crop). */
  natural: { width: number; height: number };
  label: string;
  toolbarStart?: ReactNode;
  toolbarEnd?: ReactNode;
  crop?: CropControl | null;
  /** Drawn over the image at its displayed size (annotation/redaction editors). */
  overlay?: ((box: { w: number; h: number; natural: { width: number; height: number } }) => ReactNode) | null;
  /** Background colour and border (in image pixels) shown around the image. */
  frame?: { color: string; padding: number } | null;
}

const ZOOMS = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.5, 2, 3, 4, 6, 8];
const TIERS = [512, 1024, 2048, 4096, 8192];

export function PreviewView({ backend, path, edits, natural, label, toolbarStart, toolbarEnd, crop, overlay, frame }: Props) {
  const viewport = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 800, h: 600 });
  const [zoom, setZoom] = useState<number | "fit">("fit");

  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) setBox({ w: entry.contentRect.width, h: entry.contentRect.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const pad = 48;
  const border = frame?.padding ?? 0;
  const fitScale = Math.min(1, (box.w - pad) / (natural.width + 2 * border), (box.h - pad) / (natural.height + 2 * border));
  const scale = zoom === "fit" ? Math.max(0.01, fitScale) : zoom;
  const dispW = Math.max(1, Math.round(natural.width * scale));
  const dispH = Math.max(1, Math.round(natural.height * scale));

  // Request just enough resolution for what is on screen.
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const needed = Math.max(dispW, dispH) * dpr;
  const tier = Math.min(TIERS.find((t) => t >= needed) ?? 8192, Math.max(natural.width, natural.height, 512));
  const preview = usePreview(backend, path, edits, tier);

  const step = (dir: 1 | -1) => {
    const cur = scale;
    const next = dir > 0 ? ZOOMS.find((z) => z > cur + 1e-6) : [...ZOOMS].reverse().find((z) => z < cur - 1e-6);
    setZoom(next ?? cur);
  };

  const onWheel = (e: WheelEvent) => {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    step(e.deltaY < 0 ? 1 : -1);
  };

  return (
    <div className="preview">
      <div className="preview__toolbar" role="toolbar" aria-label={`${label} preview controls`}>
        <div className="preview__group">{toolbarStart}</div>
        <div className="preview__group">
          <button type="button" className="icon-btn" onClick={() => step(-1)} aria-label="Zoom out" data-tip="Zoom out">
            <IconZoomOut />
          </button>
          <span className="preview__zoom" aria-live="polite">
            {Math.round(scale * 100)}%
          </span>
          <button type="button" className="icon-btn" onClick={() => step(1)} aria-label="Zoom in" data-tip="Zoom in">
            <IconZoomIn />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-pressed={zoom === "fit"}
            onClick={() => setZoom("fit")}
            aria-label="Fit to window"
            data-tip="Fit"
          >
            <IconFit />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-pressed={zoom === 1}
            onClick={() => setZoom(1)}
            aria-label="Actual size (100%)"
            data-tip="Actual size"
          >
            <IconActualSize />
          </button>
        </div>
        <div className="preview__group">{toolbarEnd}</div>
      </div>
      <div ref={viewport} className="preview__viewport" onWheel={onWheel}>
        <div
          className={`preview__frame${frame ? " preview__frame--filled" : ""}`}
          style={frame ? { padding: Math.round(border * scale), background: frame.color } : undefined}
        >
        <div className="preview__canvas" style={{ width: dispW, height: dispH, ...(frame ? { background: frame.color } : {}) }}>
          {preview.url ? (
            <img src={preview.url} alt={`${label} preview`} draggable={false} width={dispW} height={dispH} />
          ) : (
            <div className="preview__placeholder" />
          )}
          {crop && <CropOverlay crop={crop} width={dispW} height={dispH} />}
          {overlay?.({ w: dispW, h: dispH, natural })}
        </div>
        </div>
      </div>
      {preview.error && (
        <p className="preview__error" role="alert">
          Preview unavailable: {preview.error.message}
        </p>
      )}
      <div className="preview__caption">
        <span className="badge">{label}</span>
        <span>
          {natural.width} × {natural.height}
        </span>
        {preview.loading && <span className="preview__busy">Rendering…</span>}
      </div>
    </div>
  );
}

type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
const MIN = 0.02;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function applyCropDrag(start: CropRect, h: Handle, dx: number, dy: number): CropRect {
  let { x, y, width, height } = start;
  if (h === "move") {
    return { x: clamp(x + dx, 0, 1 - width), y: clamp(y + dy, 0, 1 - height), width, height };
  }
  if (h.includes("w")) {
    const nx = clamp(x + dx, 0, x + width - MIN);
    width += x - nx;
    x = nx;
  }
  if (h.includes("e")) width = clamp(width + dx, MIN, 1 - x);
  if (h.includes("n")) {
    const ny = clamp(y + dy, 0, y + height - MIN);
    height += y - ny;
    y = ny;
  }
  if (h.includes("s")) height = clamp(height + dy, MIN, 1 - y);
  return { x, y, width, height };
}

function CropOverlay({ crop, width, height }: { crop: CropControl; width: number; height: number }) {
  const drag = useRef<{ h: Handle; sx: number; sy: number; start: CropRect } | null>(null);
  const c = crop.value;

  const down = (e: PointerEvent<HTMLElement>) => {
    e.stopPropagation();
    const h = (e.currentTarget.dataset.handle ?? "move") as Handle;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { h, sx: e.clientX, sy: e.clientY, start: c };
  };
  const move = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d) return;
    crop.onChange(applyCropDrag(d.start, d.h, (e.clientX - d.sx) / width, (e.clientY - d.sy) / height));
  };
  const up = () => {
    drag.current = null;
  };

  // Arrow keys move the crop; Shift+arrows resize it.
  const onKey = (e: KeyboardEvent) => {
    const k = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    if (!k) return;
    e.preventDefault();
    const s = e.altKey ? 0.002 : 0.01;
    const [kx = 0, ky = 0] = k;
    crop.onChange(applyCropDrag(c, e.shiftKey ? (kx ? "e" : "s") : "move", kx * s, ky * s));
  };

  const handles: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
  return (
    <div
      className="crop"
      style={{ left: c.x * width, top: c.y * height, width: c.width * width, height: c.height * height }}
      data-handle="move"
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      tabIndex={0}
      role="group"
      aria-label={`Crop area: ${Math.round(c.width * 100)}% × ${Math.round(c.height * 100)}%. Arrow keys move, Shift+arrows resize.`}
      onKeyDown={onKey}
    >
      <div className="crop__grid" />
      {handles.map((h) => (
        <span
          key={h}
          className={`crop__handle crop__handle--${h}`}
          data-handle={h}
          onPointerDown={down}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={up}
        />
      ))}
    </div>
  );
}
