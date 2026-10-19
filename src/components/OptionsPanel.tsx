// Conversion options. Only options the capability declares are shown.

import { useId } from "react";
import type { ConvertOptions, Edits, FileInfo, OptionSpec } from "../lib/types";
import { IconAdjust, IconCrop, IconLink, IconRotateLeft, IconRotateRight, IconUnlink } from "./Icons";

interface Props {
  specs: OptionSpec[];
  file: FileInfo;
  options: ConvertOptions;
  disabled: boolean;
  editing: boolean;
  /** Hide the "preview edits" toggle (when the preview is always shown). */
  hideEditorToggle?: boolean;
  onOptions: (patch: Partial<ConvertOptions>) => void;
  onEdits: (patch: Partial<Edits>) => void;
  onResetEdits: () => void;
  onToggleEditor: () => void;
  onCrop: () => void;
}

const fill = (v: number, min: number, max: number) => ({ ["--fill" as string]: `${((v - min) / (max - min)) * 100}%` });

/** Dimensions after rotation and crop, before resize. */
export function editedSize(file: FileInfo, edits: Edits): { width: number; height: number } {
  let w = file.width ?? 1;
  let h = file.height ?? 1;
  if (edits.rotate === 90 || edits.rotate === 270) [w, h] = [h, w];
  if (edits.crop) {
    w = Math.max(1, Math.round(edits.crop.width * w));
    h = Math.max(1, Math.round(edits.crop.height * h));
  }
  return { width: w, height: h };
}

export function OptionsPanel({ specs, file, options, disabled, editing, hideEditorToggle = false, onOptions, onEdits, onResetEdits, onToggleEditor, onCrop }: Props) {
  const id = useId();
  const has = (k: string) => specs.some((o) => o.kind === k);
  const quality = specs.find((o) => o.kind === "quality");
  const e = options.edits;
  const base = editedSize(file, e);
  const r = options.resize;
  const edited = e.rotate !== 0 || e.crop !== null || e.brightness !== 0 || e.contrast !== 0;

  const setResize = (patch: Partial<NonNullable<ConvertOptions["resize"]>>) => {
    const cur = r ?? { width: base.width, height: base.height, keepAspect: true };
    const next = { ...cur, ...patch };
    if (next.keepAspect && patch.width !== undefined && patch.width !== null) {
      next.height = Math.max(1, Math.round((patch.width * base.height) / base.width));
    } else if (next.keepAspect && patch.height !== undefined && patch.height !== null) {
      next.width = Math.max(1, Math.round((patch.height * base.width) / base.height));
    }
    onOptions({ resize: next });
  };

  return (
    <fieldset className="options" disabled={disabled}>
      <legend className="sr-only">Conversion options</legend>

      {quality && quality.kind === "quality" && !options.lossless && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-q`}>
            Quality <span className="field__value">{options.quality ?? quality.default}</span>
          </label>
          <input
            id={`${id}-q`}
            type="range"
            min={quality.min}
            max={quality.max}
            value={options.quality ?? quality.default}
            style={fill(options.quality ?? quality.default, quality.min, quality.max)}
            onChange={(ev) => onOptions({ quality: Number(ev.target.value) })}
          />
          <span className="field__hint">Higher keeps more detail and makes larger files.</span>
        </div>
      )}

      {has("lossless") && (
        <div className="row">
          <span id={`${id}-ll`} className="row__label">
            Lossless
            <span className="field__hint">Exact pixels, larger file</span>
          </span>
          <button
            type="button"
            role="switch"
            aria-checked={options.lossless}
            aria-labelledby={`${id}-ll`}
            className="switch"
            onClick={() => onOptions({ lossless: !options.lossless })}
          />
        </div>
      )}

      {has("background") && (
        <div className="row">
          <label className="row__label" htmlFor={`${id}-bg`}>
            Background
            <span className="field__hint">
              {file.hasAlpha ? "Fills transparent areas" : "Used if the image has transparency"}
            </span>
          </label>
          <span className="color">
            <input
              id={`${id}-bg`}
              type="color"
              value={options.background ?? "#ffffff"}
              onChange={(ev) => onOptions({ background: ev.target.value })}
            />
            <span className="mono">{(options.background ?? "#ffffff").toUpperCase()}</span>
          </span>
        </div>
      )}

      {has("resize") && (
        <div className="field">
          <div className="row">
            <span id={`${id}-rs`} className="row__label">
              Resize
              <span className="field__hint">
                {r ? "Lanczos resampling" : `Keeps ${base.width} × ${base.height}`}
              </span>
            </span>
            <button
              type="button"
              role="switch"
              aria-checked={r !== null}
              aria-labelledby={`${id}-rs`}
              className="switch"
              onClick={() => onOptions({ resize: r ? null : { width: base.width, height: base.height, keepAspect: true } })}
            />
          </div>
          {r && (
            <div className="dims">
              <label>
                <span className="sr-only">Width in pixels</span>
                <input
                  className="input mono"
                  type="number"
                  min={1}
                  max={30000}
                  value={r.width ?? ""}
                  onChange={(ev) => setResize({ width: ev.target.value ? Math.max(1, Number(ev.target.value)) : null })}
                  aria-label="Width in pixels"
                />
              </label>
              <button
                type="button"
                className="icon-btn"
                aria-pressed={r.keepAspect}
                aria-label={r.keepAspect ? "Aspect ratio locked" : "Aspect ratio unlocked"}
                data-tip={r.keepAspect ? "Aspect ratio locked" : "Aspect ratio unlocked"}
                onClick={() => setResize({ keepAspect: !r.keepAspect })}
              >
                {r.keepAspect ? <IconLink /> : <IconUnlink />}
              </button>
              <label>
                <span className="sr-only">Height in pixels</span>
                <input
                  className="input mono"
                  type="number"
                  min={1}
                  max={30000}
                  value={r.height ?? ""}
                  onChange={(ev) => setResize({ height: ev.target.value ? Math.max(1, Number(ev.target.value)) : null })}
                  aria-label="Height in pixels"
                />
              </label>
              <span className="dims__unit">px</span>
            </div>
          )}
        </div>
      )}

      {has("brightness_contrast") && (
        <div className="edit-group">
          <div className="edit-group__head">
            <span className="section-label">Edit</span>
            <div className="edit-group__tools">
              {has("rotate") && (
                <>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label="Rotate left"
                    data-tip="Rotate left"
                    onClick={() => onEdits({ rotate: (((e.rotate + 270) % 360) as Edits["rotate"]), crop: null })}
                  >
                    <IconRotateLeft />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label="Rotate right"
                    data-tip="Rotate right"
                    onClick={() => onEdits({ rotate: (((e.rotate + 90) % 360) as Edits["rotate"]), crop: null })}
                  >
                    <IconRotateRight />
                  </button>
                </>
              )}
              {has("crop") && (
                <button
                  type="button"
                  className="icon-btn"
                  aria-pressed={e.crop !== null}
                  aria-label="Crop"
                  data-tip="Crop"
                  onClick={onCrop}
                >
                  <IconCrop />
                </button>
              )}
              {!hideEditorToggle && (
              <button
                type="button"
                className="icon-btn"
                aria-pressed={editing}
                aria-label={editing ? "Close preview" : "Open preview"}
                data-tip={editing ? "Close preview" : "Preview edits"}
                data-tip-pos="left"
                onClick={onToggleEditor}
              >
                <IconAdjust />
              </button>
              )}
            </div>
          </div>
          <Slider id={`${id}-b`} label="Brightness" value={e.brightness} onChange={(v) => onEdits({ brightness: v })} />
          <Slider id={`${id}-c`} label="Contrast" value={e.contrast} onChange={(v) => onEdits({ contrast: v })} />
          {edited && (
            <p className="edit-summary">
              <span>
                {[
                  e.rotate ? `Rotated ${e.rotate}°` : null,
                  e.crop ? `Cropped to ${base.width} × ${base.height}` : null,
                  e.brightness || e.contrast ? "Tone adjusted" : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
              <button type="button" className="link" onClick={onResetEdits}>
                Reset
              </button>
            </p>
          )}
        </div>
      )}
    </fieldset>
  );
}

function Slider({ id, label, value, onChange }: { id: string; label: string; value: number; onChange: (v: number) => void }) {
  return (
    <div className="field">
      <label className="field__label" htmlFor={id}>
        {label}
        <span className="field__value">{value > 0 ? `+${value}` : value}</span>
      </label>
      <input
        id={id}
        type="range"
        min={-100}
        max={100}
        value={value}
        style={fill(value, -100, 100)}
        onChange={(ev) => onChange(Number(ev.target.value))}
        onDoubleClick={() => onChange(0)}
      />
    </div>
  );
}
