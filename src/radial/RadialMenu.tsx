// The radial selector: choices (operations, or output formats for Convert)
// arranged in a ring around the selected file. Three equivalent ways to choose:
//   • click (or tap) a segment;
//   • drag the file in the centre onto a segment and release;
//   • keyboard: arrow keys move around the ring, Enter/Space choose.
// Items come only from the registry data passed in by the caller.

import { useCallback, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import { clampOffset, hitSegment, layoutSegments, polar, ringMetrics, sectorPath, stepIndex } from "./geometry";

export interface RadialItem {
  id: string;
  label: string;
  icon: ReactNode;
  available: boolean;
  /** Accessible name, e.g. "Convert to JPEG". */
  action: string;
  /** Why the item is unavailable. */
  reason?: string | null;
}

export interface RadialMenuProps {
  items: RadialItem[];
  selected: string | null;
  onSelect: (id: string) => void;
  /** Accessible name of the ring, e.g. "Operations". */
  label: string;
  /** Square side length in CSS pixels. */
  size: number;
  /** Disable selection (e.g. while converting). */
  busy?: boolean;
  /** Content of the central orb (file thumbnail, progress…). */
  children: ReactNode;
  /** Shown under the orb while dragging over a segment. */
  sourceLabel: string;
}

const DRAG_THRESHOLD = 6;

interface Drag {
  pointerId: number;
  startX: number;
  startY: number;
  originX: number;
  originY: number;
  active: boolean;
}

export function RadialMenu({ items: capabilities, selected, onSelect, label, size, busy = false, children, sourceLabel }: RadialMenuProps) {
  const m = ringMetrics(size);
  const segments = useMemo(() => layoutSegments(capabilities.length, capabilities.length > 6 ? 3 : 4), [capabilities.length]);
  const selectedIndex = capabilities.findIndex((c) => c.id === selected);
  const [focused, setFocusIndex] = useState<number | null>(null);
  // Roving tab stop: last focused segment, else the selection, else the first.
  const focusIndex = focused !== null && focused < capabilities.length ? focused : Math.max(0, selectedIndex);
  const [hover, setHover] = useState<number | null>(null);
  const [armed, setArmed] = useState<number | null>(null);
  const [offset, setOffset] = useState<{ x: number; y: number } | null>(null);
  const drag = useRef<Drag | null>(null);
  const segRefs = useRef<(SVGGElement | null)[]>([]);
  const rootRef = useRef<HTMLDivElement>(null);

  // (The parent remounts this component per source file, which resets
  // hover/drag state.)

  const choose = useCallback(
    (i: number) => {
      const cap = capabilities[i];
      if (!cap || !cap.available || busy) return;
      onSelect(cap.id);
    },
    [capabilities, busy, onSelect],
  );

  const onKeyDown = (e: KeyboardEvent<SVGGElement>, i: number) => {
    const n = capabilities.length;
    let next: number;
    switch (e.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = stepIndex(i, 1, n);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = stepIndex(i, -1, n);
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = n - 1;
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        choose(i);
        return;
      default:
        if (/^[1-9]$/.test(e.key) && Number(e.key) <= n) {
          e.preventDefault();
          choose(Number(e.key) - 1);
        }
        return;
    }
    e.preventDefault();
    setFocusIndex(next);
    segRefs.current[next]?.focus();
  };

  // ---- Drag the file onto a segment ----

  const centerOnScreen = () => {
    const r = rootRef.current?.getBoundingClientRect();
    return r ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : { x: 0, y: 0 };
  };

  const endDrag = (commit: boolean) => {
    const d = drag.current;
    drag.current = null;
    if (d?.active && commit && armed !== null) choose(armed);
    setArmed(null);
    setOffset(null);
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (busy || e.button !== 0 || capabilities.length === 0) return;
    const c = centerOnScreen();
    drag.current = { pointerId: e.pointerId, startX: e.clientX, startY: e.clientY, originX: c.x, originY: c.y, active: false };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || d.pointerId !== e.pointerId) return;
    if (!d.active && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return;
    d.active = true;
    const dx = e.clientX - d.originX;
    const dy = e.clientY - d.originY;
    const hit = hitSegment(dx, dy, capabilities.length, m.inner * 0.9);
    setArmed(hit !== null && capabilities[hit]?.available ? hit : null);
    setOffset(clampOffset(dx, dy, m.labelRadius));
  };

  const onKeyDuringDrag = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape" && drag.current) {
      e.stopPropagation();
      endDrag(false);
    }
  };

  const dragging = offset !== null;
  const armedCap = armed !== null ? capabilities[armed] : null;

  return (
    <div
      ref={rootRef}
      className={`radial${busy ? " radial--busy" : ""}${dragging ? " radial--dragging" : ""}`}
      style={{ width: m.size, height: m.size }}
      onKeyDown={onKeyDuringDrag}
    >
      <svg className="radial__svg" width={m.size} height={m.size} viewBox={`0 0 ${m.size} ${m.size}`}>
        <defs>
          <radialGradient id="seg-fill" cx="50%" cy="50%" r="50%">
            <stop offset={m.inner / m.outer} stopColor="#1a1e28" />
            <stop offset="1" stopColor="#14171f" />
          </radialGradient>
          <radialGradient id="seg-active" cx="50%" cy="50%" r="50%">
            <stop offset={m.inner / m.outer} stopColor="rgba(143,162,255,0.30)" />
            <stop offset="1" stopColor="rgba(143,162,255,0.12)" />
          </radialGradient>
          <pattern id="seg-hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <line x1="0" y1="0" x2="0" y2="6" stroke="rgba(255,255,255,0.05)" strokeWidth="2" />
          </pattern>
        </defs>

        {/* Faint guide circles. */}
        <circle className="radial__guide" cx={m.center} cy={m.center} r={m.outer + 4} />
        <circle className="radial__guide" cx={m.center} cy={m.center} r={m.inner - 6} />

        <g role="radiogroup" aria-label={label} aria-disabled={busy || undefined}>
          {capabilities.map((cap, i) => {
            const seg = segments[i];
            if (!seg) return null;
            const at = polar(m.center, m.center, m.labelRadius, seg.mid);
            const push = polar(0, 0, 7, seg.mid);
            const isSelected = i === selectedIndex;
            const isArmed = i === armed;
            const isHover = i === hover && !dragging;
            const iconSize = Math.max(18, Math.min(26, m.size * 0.045));
            const cls = [
              "seg",
              isSelected && "seg--selected",
              isArmed && "seg--armed",
              isHover && "seg--hover",
              !cap.available && "seg--unavailable",
            ]
              .filter(Boolean)
              .join(" ");
            const desc = cap.available ? cap.action : `${cap.label} unavailable: ${cap.reason ?? "not supported for this file"}`;
            return (
              <g
                key={cap.id}
                ref={(el) => {
                  segRefs.current[i] = el;
                }}
                className={cls}
                role="radio"
                aria-checked={isSelected}
                aria-disabled={!cap.available || busy || undefined}
                aria-label={desc}
                data-id={cap.id}
                tabIndex={i === focusIndex ? 0 : -1}
                style={{ ["--dx" as string]: `${push.x}px`, ["--dy" as string]: `${push.y}px`, ["--i" as string]: i }}
                onClick={() => choose(i)}
                onKeyDown={(e) => onKeyDown(e, i)}
                onFocus={() => setFocusIndex(i)}
                onPointerEnter={() => setHover(i)}
                onPointerLeave={() => setHover((h) => (h === i ? null : h))}
              >
                {/* Native tooltips linger in WebKitGTK after the ring closes, so
                    only unavailable items (which explain why) get one. */}
                {!cap.available && <title>{desc}</title>}
                <g className="seg__inner">
                  <path className="seg__shape" d={sectorPath(m.center, m.center, m.inner, m.outer, seg.start, seg.end)} />
                  {!cap.available && (
                    <path className="seg__hatch" d={sectorPath(m.center, m.center, m.inner, m.outer, seg.start, seg.end)} />
                  )}
                  <svg
                    className="seg__icon"
                    x={at.x - iconSize / 2}
                    y={at.y - iconSize / 2 - 9}
                    width={iconSize}
                    height={iconSize}
                    viewBox="0 0 24 24"
                    overflow="visible"
                  >
                    {cap.icon}
                  </svg>
                  <text className="seg__label" x={at.x} y={at.y + iconSize / 2 + 6} textAnchor="middle">
                    {cap.label}
                  </text>
                  {i < 9 && (
                    <text className="seg__key" x={at.x} y={at.y + iconSize / 2 + 19} textAnchor="middle" aria-hidden="true">
                      {i + 1}
                    </text>
                  )}
                </g>
              </g>
            );
          })}
        </g>

        {dragging && offset && (
          <line
            className={`radial__tether${armedCap ? " radial__tether--armed" : ""}`}
            x1={m.center}
            y1={m.center}
            x2={m.center + offset.x}
            y2={m.center + offset.y}
          />
        )}
      </svg>

      {/* The orb: file representation and drag handle. */}
      <div
        className="radial__orb"
        style={{
          width: m.orb * 2,
          height: m.orb * 2,
          transform: offset
            ? `translate(-50%, -50%) translate(${offset.x}px, ${offset.y}px) scale(0.55)`
            : "translate(-50%, -50%)",
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={() => endDrag(true)}
        onPointerCancel={() => endDrag(false)}
        onLostPointerCapture={() => drag.current && endDrag(false)}
        aria-hidden="true"
      >
        {children}
      </div>

      {dragging && (
        <div className="radial__drag-hint" aria-hidden="true">
          {armedCap ? (
            <>
              Release: <strong>{sourceLabel}</strong> → <strong>{armedCap.label}</strong>
            </>
          ) : (
            "Drag onto a format"
          )}
        </div>
      )}
    </div>
  );
}
