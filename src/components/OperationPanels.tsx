// Settings panels for operations that don't use the generic OptionsPanel.

import { useEffect, useId, useState } from "react";
import { toRadialError, type Backend } from "../lib/backend";
import { formatBytes } from "../lib/format";
import type {
  CropRect,
  FileInfo,
  MetadataEdit,
  MetadataReport,
  Operation,
  PageRegion,
  RadialError,
  Shape,
  ShapeKind,
} from "../lib/types";
import { IconAlert, IconBack, IconCrop, IconRotateLeft, IconRotateRight, IconTrash } from "./Icons";

const fill = (v: number, min: number, max: number) => ({ ["--fill" as string]: `${((v - min) / (max - min)) * 100}%` });

// ---------------------------------------------------------------- Compress

export function CompressPanel({
  op,
  file,
  quality,
  maxDimension,
  onQuality,
  onMaxDimension,
}: {
  op: Operation;
  file: FileInfo;
  quality: number | null;
  maxDimension: number | null;
  onQuality: (q: number) => void;
  onMaxDimension: (v: number | null) => void;
}) {
  const id = useId();
  const q = op.options.find((o) => o.kind === "quality");
  const longest = Math.max(file.width ?? 0, file.height ?? 0);
  const lossless = op.options.some((o) => o.kind === "optimize");
  const pdf = file.format === "pdf";
  return (
    <div className="options">
      <p className="field__hint">
        Original: <strong className="field__value">{formatBytes(file.size)}</strong>. RADIAL saves the result only if
        it is actually smaller.
      </p>
      {q && q.kind === "quality" && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-q`}>
            Quality <span className="field__value">{quality ?? q.default}</span>
          </label>
          <input
            id={`${id}-q`}
            type="range"
            min={q.min}
            max={q.max}
            value={quality ?? q.default}
            style={fill(quality ?? q.default, q.min, q.max)}
            onChange={(e) => onQuality(Number(e.target.value))}
          />
          <span className="field__hint">
            {pdf
              ? "Photos inside the PDF are re-encoded at this quality. Text and drawings are not affected."
              : "Lower quality gives a smaller file with more visible artefacts."}
          </span>
        </div>
      )}
      {lossless && <p className="note">Lossless optimisation: the pixels stay exactly the same.</p>}
      {op.options.some((o) => o.kind === "max_dimension") && (
        <div className="field">
          <div className="row">
            <span id={`${id}-md`} className="row__label">
              {pdf ? "Limit photo size" : "Limit size"}
              <span className="field__hint">
                {maxDimension
                  ? `Longest side ${maxDimension} px`
                  : pdf
                    ? "Photos keep their resolution"
                    : `Keeps ${file.width} × ${file.height}`}
              </span>
            </span>
            <button
              type="button"
              role="switch"
              aria-checked={maxDimension !== null}
              aria-labelledby={`${id}-md`}
              className="switch"
              onClick={() =>
                onMaxDimension(maxDimension ? null : pdf ? 1600 : Math.min(2048, Math.max(1, Math.round(longest / 2))))
              }
            />
          </div>
          {maxDimension !== null && (
            <div className="dims">
              <input
                className="input mono"
                type="number"
                min={1}
                max={30000}
                value={maxDimension}
                aria-label="Longest side in pixels"
                onChange={(e) => onMaxDimension(e.target.value ? Math.max(1, Number(e.target.value)) : 1)}
              />
              <span className="dims__unit">px longest side</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Crop

export function CropPanel({
  crop,
  rotate,
  onRotate,
  onStartCrop,
  onClear,
  pdf = null,
}: {
  crop: CropRect | null;
  rotate: number;
  onRotate: (deg: 0 | 90 | 180 | 270) => void;
  onStartCrop: () => void;
  onClear: () => void;
  /** PDFs: page count and whether the crop applies to every page. */
  pdf?: { pages: number; page: number; allPages: boolean; onAllPages: (v: boolean) => void } | null;
}) {
  const id = useId();
  return (
    <div className="options">
      <p className="field__hint">Drag the handles on the {pdf ? "page" : "image"} to choose the area to keep.</p>
      <div className="button-row">
        <button type="button" className="btn" onClick={onStartCrop}>
          <IconCrop />
          {crop ? "Adjust area" : "Choose area"}
        </button>
        {crop && (
          <button type="button" className="btn btn--ghost" onClick={onClear}>
            Clear
          </button>
        )}
      </div>
      {pdf && pdf.pages > 1 && (
        <div className="row">
          <span id={`${id}-all`} className="row__label">
            Apply to all pages
            <span className="field__hint">
              {pdf.allPages ? `Same area on all ${pdf.pages} pages` : `Only page ${pdf.page + 1}`}
            </span>
          </span>
          <button
            type="button"
            role="switch"
            aria-checked={pdf.allPages}
            aria-labelledby={`${id}-all`}
            className="switch"
            onClick={() => pdf.onAllPages(!pdf.allPages)}
          />
        </div>
      )}
      {!pdf && (
      <div className="row">
        <span className="row__label">Rotate first</span>
        <div className="edit-group__tools">
          <button type="button" className="icon-btn" aria-label="Rotate left" data-tip="Rotate left" onClick={() => onRotate(((rotate + 270) % 360) as 0)}>
            <IconRotateLeft />
          </button>
          <button type="button" className="icon-btn" aria-label="Rotate right" data-tip="Rotate right" data-tip-pos="left" onClick={() => onRotate(((rotate + 90) % 360) as 0)}>
            <IconRotateRight />
          </button>
        </div>
      </div>
      )}
      {crop && (
        <p className="edit-summary">
          <span>
            Keeping {Math.round(crop.width * 100)}% × {Math.round(crop.height * 100)}% of the {pdf ? "page" : "image"}
          </span>
        </p>
      )}
      {pdf && (
        <p className="note">
          <IconAlert />
          Cropping hides the rest of the page; it doesn't delete it. Use Redact to remove sensitive content.
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Pages

/** Previous/next page control for PDFs. */
export function PageNav({ page, pages, onPage }: { page: number; pages: number; onPage: (p: number) => void }) {
  if (pages <= 1) return null;
  return (
    <div className="page-nav" role="group" aria-label="Page">
      <button
        type="button"
        className="icon-btn"
        aria-label="Previous page"
        disabled={page <= 0}
        onClick={() => onPage(page - 1)}
      >
        <IconBack />
      </button>
      <span className="page-nav__label" aria-live="polite">
        Page {page + 1} of {pages}
      </span>
      <button
        type="button"
        className="icon-btn page-nav__next"
        aria-label="Next page"
        disabled={page >= pages - 1}
        onClick={() => onPage(page + 1)}
      >
        <IconBack />
      </button>
    </div>
  );
}

/** PDF → image export: page and resolution. */
export function PdfExportOptions({
  pages,
  page,
  dpi,
  dpiSpec,
  onPage,
  onDpi,
}: {
  pages: number;
  page: number;
  dpi: number | null;
  dpiSpec: { min: number; max: number; default: number } | null;
  onPage: (p: number) => void;
  onDpi: (d: number) => void;
}) {
  const id = useId();
  const value = dpi ?? dpiSpec?.default ?? 150;
  return (
    <div className="options">
      {pages > 1 && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-page`}>
            Page
          </label>
          <div className="dims">
            <input
              id={`${id}-page`}
              className="input mono"
              type="number"
              min={1}
              max={pages}
              value={page + 1}
              onChange={(e) => onPage(Number(e.target.value) - 1)}
            />
            <span className="dims__unit">of {pages}</span>
          </div>
        </div>
      )}
      {dpiSpec && (
        <div className="field">
          <span className="field__label" id={`${id}-dpi`}>
            Resolution <span className="field__value">{value} DPI</span>
          </span>
          <div className="segmented" role="radiogroup" aria-labelledby={`${id}-dpi`}>
            {[72, 150, 300]
              .filter((d) => d >= dpiSpec.min && d <= dpiSpec.max)
              .map((d) => (
                <button key={d} type="button" role="radio" aria-checked={value === d} onClick={() => onDpi(d)}>
                  {d === 72 ? "Screen" : d === 150 ? "Standard" : "Print"}
                </button>
              ))}
          </div>
          <span className="field__hint">Higher resolution gives a sharper, larger image.</span>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Annotate

export interface AnnotateTool {
  kind: ShapeKind;
  color: string;
  stroke: number;
  size: number;
  filled: boolean;
  text: string;
}

export const DEFAULT_TOOL: AnnotateTool = { kind: "arrow", color: "#ff3b30", stroke: 0.008, size: 0.05, filled: false, text: "" };

const SWATCHES = ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#0a84ff", "#ffffff", "#111111"];
const TOOLS: { kind: ShapeKind; label: string }[] = [
  { kind: "arrow", label: "Arrow" },
  { kind: "line", label: "Line" },
  { kind: "rect", label: "Box" },
  { kind: "ellipse", label: "Ellipse" },
  { kind: "text", label: "Text" },
];

export function AnnotatePanel({
  tool,
  shapes,
  onTool,
  onShapes,
}: {
  tool: AnnotateTool;
  shapes: Shape[];
  onTool: (t: AnnotateTool) => void;
  onShapes: (s: Shape[]) => void;
}) {
  const id = useId();
  const set = (patch: Partial<AnnotateTool>) => onTool({ ...tool, ...patch });
  return (
    <div className="options">
      <div className="segmented segmented--wide" role="radiogroup" aria-label="Annotation tool">
        {TOOLS.map((t) => (
          <button key={t.kind} type="button" role="radio" aria-checked={tool.kind === t.kind} onClick={() => set({ kind: t.kind })}>
            {t.label}
          </button>
        ))}
      </div>
      {tool.kind === "text" && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-t`}>
            Text to place
          </label>
          <input
            id={`${id}-t`}
            className="input"
            value={tool.text}
            placeholder="Type, then click on the image"
            onChange={(e) => set({ text: e.target.value })}
          />
        </div>
      )}
      <div className="field">
        <span className="field__label">Colour</span>
        <div className="swatches" role="radiogroup" aria-label="Colour">
          {SWATCHES.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={tool.color === c}
              aria-label={c}
              className="swatch"
              style={{ background: c }}
              onClick={() => set({ color: c })}
            />
          ))}
        </div>
      </div>
      {tool.kind === "text" ? (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-s`}>
            Text size <span className="field__value">{Math.round(tool.size * 100)}</span>
          </label>
          <input id={`${id}-s`} type="range" min={2} max={20} value={Math.round(tool.size * 100)} style={fill(tool.size * 100, 2, 20)} onChange={(e) => set({ size: Number(e.target.value) / 100 })} />
        </div>
      ) : (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-w`}>
            Line width <span className="field__value">{Math.round(tool.stroke * 1000)}</span>
          </label>
          <input id={`${id}-w`} type="range" min={2} max={40} value={Math.round(tool.stroke * 1000)} style={fill(tool.stroke * 1000, 2, 40)} onChange={(e) => set({ stroke: Number(e.target.value) / 1000 })} />
        </div>
      )}
      {(tool.kind === "rect" || tool.kind === "ellipse") && (
        <div className="row">
          <span id={`${id}-f`} className="row__label">
            Filled
          </span>
          <button type="button" role="switch" aria-checked={tool.filled} aria-labelledby={`${id}-f`} className="switch" onClick={() => set({ filled: !tool.filled })} />
        </div>
      )}
      <p className="field__hint">
        {tool.kind === "text" ? "Click on the image to place the text." : "Drag on the image to draw."} Annotations become part of the saved image.
      </p>
      {shapes.length > 0 && (
        <div className="shape-list">
          <div className="section-head">
            <span className="field__label">{shapes.length} annotation{shapes.length === 1 ? "" : "s"}</span>
            <div className="button-row button-row--tight">
              <button type="button" className="link" onClick={() => onShapes(shapes.slice(0, -1))}>
                Undo
              </button>
              <button type="button" className="link" onClick={() => onShapes([])}>
                Clear all
              </button>
            </div>
          </div>
          <ul>
            {shapes.map((s, i) => (
              <li key={i}>
                <span className="swatch swatch--sm" style={{ background: s.color }} aria-hidden="true" />
                <span className="truncate">{s.kind === "text" ? `“${s.text}”` : TOOLS.find((t) => t.kind === s.kind)?.label}</span>
                <button type="button" className="icon-btn" aria-label={`Remove annotation ${i + 1}`} onClick={() => onShapes(shapes.filter((_, j) => j !== i))}>
                  <IconTrash />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Redact

/** Redact for PDFs: areas on any page plus text to find everywhere. */
export function PdfRedactPanel({
  regions,
  terms,
  pages,
  onRegions,
  onTerms,
  onGoToPage,
}: {
  regions: PageRegion[];
  terms: string[];
  pages: number;
  onRegions: (r: PageRegion[]) => void;
  onTerms: (t: string[]) => void;
  onGoToPage: (p: number) => void;
}) {
  const id = useId();
  const [draft, setDraft] = useState(terms.join("\n"));
  const pagesWith = new Set(regions.map((r) => r.page)).size;
  return (
    <div className="options">
      <p className="field__hint">
        Drag on the page to cover areas{pages > 1 ? " (use the page arrows to move between pages)" : ""}, and/or enter
        text to remove wherever it appears.
      </p>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-terms`}>
          Find and remove text
        </label>
        <textarea
          id={`${id}-terms`}
          className="input"
          rows={3}
          value={draft}
          placeholder={"One per line, e.g.\nACME Corporation\nDE89 3704"}
          onChange={(e) => {
            setDraft(e.target.value);
            onTerms(
              e.target.value
                .split("\n")
                .map((t) => t.trim())
                .filter(Boolean),
            );
          }}
        />
        <span className="field__hint">Not case-sensitive. Scanned pages have no text to search; mark them by hand.</span>
      </div>
      <p className="note">
        <IconAlert />
        Each page with a redaction is saved as an image with the areas blacked out, so its text can no longer be
        selected or recovered. Other pages stay as they are. Document metadata is removed.
      </p>
      {regions.length > 0 && (
        <div className="shape-list">
          <div className="section-head">
            <span className="field__label">
              {regions.length} area{regions.length === 1 ? "" : "s"}
              {pages > 1 ? ` on ${pagesWith} page${pagesWith === 1 ? "" : "s"}` : ""}
            </span>
            <button type="button" className="link" onClick={() => onRegions([])}>
              Clear all
            </button>
          </div>
          <ul>
            {regions.map((r, i) => (
              <li key={i}>
                <span className="swatch swatch--sm" style={{ background: "#000" }} aria-hidden="true" />
                <button type="button" className="link truncate" onClick={() => onGoToPage(r.page)}>
                  Area {i + 1}
                  {pages > 1 ? ` · page ${r.page + 1}` : ""}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Remove area ${i + 1}`}
                  onClick={() => onRegions(regions.filter((_, j) => j !== i))}
                >
                  <IconTrash />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export function RedactPanel({ regions, onRegions }: { regions: CropRect[]; onRegions: (r: CropRect[]) => void }) {
  return (
    <div className="options">
      <p className="field__hint">Drag on the image to cover each area you want removed. Areas are drawn as solid black.</p>
      <p className="note">
        <IconAlert />
        The saved copy has the pixels under each box replaced with black, and all metadata removed. The original is not
        changed.
      </p>
      {regions.length > 0 && (
        <div className="shape-list">
          <div className="section-head">
            <span className="field__label">
              {regions.length} area{regions.length === 1 ? "" : "s"} marked
            </span>
            <button type="button" className="link" onClick={() => onRegions([])}>
              Clear all
            </button>
          </div>
          <ul>
            {regions.map((r, i) => (
              <li key={i}>
                <span className="swatch swatch--sm" style={{ background: "#000" }} aria-hidden="true" />
                <span className="truncate">
                  Area {i + 1} · {Math.round(r.width * 100)}% × {Math.round(r.height * 100)}%
                </span>
                <button type="button" className="icon-btn" aria-label={`Remove area ${i + 1}`} onClick={() => onRegions(regions.filter((_, j) => j !== i))}>
                  <IconTrash />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- Background

const BG_SWATCHES = ["#ffffff", "#000000", "#f2f2f2", "#0b1f3a", "#e8f0ff", "#fff4e0"];

export function BackgroundPanel({
  file,
  color,
  padding,
  onColor,
  onPadding,
}: {
  file: FileInfo;
  color: string;
  padding: number;
  onColor: (c: string) => void;
  onPadding: (p: number) => void;
}) {
  const id = useId();
  const max = Math.max(50, Math.round(Math.max(file.width ?? 0, file.height ?? 0) / 4));
  return (
    <div className="options">
      {!file.hasAlpha && <p className="note">This image has no transparency. Add a border to frame it.</p>}
      <div className="row">
        <label className="row__label" htmlFor={`${id}-c`}>
          Colour
          <span className="field__hint">{file.hasAlpha ? "Fills transparent areas and the border" : "Fills the border"}</span>
        </label>
        <span className="color">
          <input id={`${id}-c`} type="color" value={color} onChange={(e) => onColor(e.target.value)} />
          <span className="mono">{color.toUpperCase()}</span>
        </span>
      </div>
      <div className="swatches" role="radiogroup" aria-label="Background presets">
        {BG_SWATCHES.map((c) => (
          <button key={c} type="button" role="radio" aria-checked={color === c} aria-label={c} className="swatch" style={{ background: c }} onClick={() => onColor(c)} />
        ))}
      </div>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-p`}>
          Border <span className="field__value">{padding} px</span>
        </label>
        <input id={`${id}-p`} type="range" min={0} max={max} value={padding} style={fill(padding, 0, max)} onChange={(e) => onPadding(Number(e.target.value))} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- Metadata

export function MetadataPanel({
  backend,
  path,
  edit,
  onEdit,
}: {
  backend: Backend | null;
  path: string;
  edit: MetadataEdit;
  onEdit: (patch: Partial<MetadataEdit>) => void;
}) {
  const id = useId();
  const [loaded, setLoaded] = useState<{ path: string; report: MetadataReport | null; error: RadialError | null } | null>(null);
  useEffect(() => {
    if (!backend) return;
    let alive = true;
    backend
      .readMetadata(path)
      .then((report) => alive && setLoaded({ path, report, error: null }))
      .catch((e: unknown) => alive && setLoaded({ path, report: null, error: toRadialError(e) }));
    return () => {
      alive = false;
    };
  }, [backend, path]);

  if (!loaded || loaded.path !== path) return <p className="muted">Reading metadata…</p>;
  if (loaded.error || !loaded.report) return <p className="problem">{loaded.error?.message ?? "Couldn't read metadata."}</p>;
  const r = loaded.report;
  const value = (key: string, original: string | null) => (key in edit.fields ? edit.fields[key] : (original ?? ""));

  return (
    <div className="options">
      {r.hasLocation && (
        <p className="note note--warning">
          <IconAlert />
          This file records where it was taken (GPS location).
        </p>
      )}
      {r.fields.map((f) => (
        <div className="field" key={f.key}>
          <label className="field__label" htmlFor={`${id}-${f.key}`}>
            {f.label}
            {f.key in edit.fields && <span className="field__value">edited</span>}
          </label>
          <input
            id={`${id}-${f.key}`}
            className="input"
            value={value(f.key, f.value)}
            disabled={!f.editable || edit.stripAll}
            placeholder={f.editable ? "Not set" : "Can't be edited for this format"}
            onChange={(e) => {
              const rest = Object.fromEntries(Object.entries(edit.fields).filter(([k]) => k !== f.key));
              // Typing the original value back means "unchanged".
              onEdit({ fields: e.target.value === (f.value ?? "") ? rest : { ...rest, [f.key]: e.target.value } });
            }}
          />
        </div>
      ))}
      {r.canStrip && (
        <div className="row">
          <span id={`${id}-strip`} className="row__label">
            Remove all metadata
            <span className="field__hint">Location, camera details, text and comments</span>
          </span>
          <button type="button" role="switch" aria-checked={edit.stripAll} aria-labelledby={`${id}-strip`} className="switch" onClick={() => onEdit({ stripAll: !edit.stripAll })} />
        </div>
      )}
      {r.details.length > 0 && (
        <details className="limits" open>
          <summary>Also in this file</summary>
          <dl className="meta-details">
            {r.details.map((d, i) => (
              <div key={i}>
                <dt>{d.label}</dt>
                <dd className="selectable">{d.value}</dd>
              </div>
            ))}
          </dl>
        </details>
      )}
      {r.notes.map((n) => (
        <p className="field__hint" key={n}>
          {n}
        </p>
      ))}
    </div>
  );
}
