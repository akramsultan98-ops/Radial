// The workflow as a pure reducer:
//   empty → (file) → choose → (operation [+ format]) → configure → running → done | failed
// A rejected file (unreadable/unsupported) parks in `rejected`.

import type {
  ConvertOptions,
  CropRect,
  Edits,
  FileInfo,
  Format,
  ImportResult,
  JobStage,
  MetadataEdit,
  Operation,
  OperationId,
  OperationOutcome,
  OutputSuggestion,
  RadialError,
  Settings,
  Shape,
} from "../lib/types";
import { EMPTY_METADATA_EDIT, NO_EDITS } from "../lib/types";

export type Phase = "empty" | "rejected" | "choose" | "configure" | "converting" | "done" | "failed";

export type StageView = "radial" | "edit" | "result";

export interface Output {
  destDir: string;
  stem: string;
}

export interface State {
  phase: Phase;
  /** Everything imported in the last drop/pick; one is active at a time. */
  tray: ImportResult[];
  active: number;
  operations: Operation[] | null;
  operation: OperationId | null;
  /** Convert only. */
  target: Format | null;
  options: ConvertOptions;
  metadata: MetadataEdit;
  /** Compress: longest side in pixels. */
  maxDimension: number | null;
  output: Output | null;
  job: { id: string; stage: JobStage } | null;
  outcome: OperationOutcome | null;
  error: RadialError | null;
  view: StageView;
  /** A one-off message about the last import (e.g. skipped files). */
  notice: string | null;
}

export type Action =
  | { type: "imported"; items: ImportResult[] }
  | { type: "select_item"; index: number }
  | { type: "operations_loaded"; operations: Operation[] }
  | { type: "operations_failed"; error: RadialError }
  | { type: "operation_chosen"; operation: OperationId; settings: Settings }
  | { type: "operation_cleared" }
  | { type: "target_chosen"; target: Format; settings: Settings }
  | { type: "target_cleared" }
  | { type: "options_changed"; patch: Partial<ConvertOptions> }
  | { type: "edits_changed"; patch: Partial<Edits> }
  | { type: "edits_reset" }
  | { type: "shapes_changed"; shapes: Shape[] }
  | { type: "regions_changed"; regions: CropRect[] }
  | { type: "metadata_changed"; patch: Partial<MetadataEdit> }
  | { type: "max_dimension_changed"; value: number | null }
  | { type: "output_suggested"; suggestion: OutputSuggestion }
  | { type: "output_changed"; patch: Partial<Output> }
  | { type: "view_changed"; view: StageView }
  | { type: "convert_started"; jobId: string }
  | { type: "job_stage"; jobId: string; stage: JobStage }
  | { type: "convert_succeeded"; jobId: string; outcome: OperationOutcome }
  | { type: "convert_failed"; jobId: string; error: RadialError }
  | {
      type: "retry_loaded";
      item: ImportResult;
      operation: OperationId;
      target: Format;
      output: Output;
      options: ConvertOptions;
    }
  | { type: "convert_another" }
  | { type: "dismiss_error" }
  | { type: "dismiss_notice" }
  | { type: "reset" };

export const defaultOptions = (): ConvertOptions => ({
  quality: null,
  lossless: false,
  background: null,
  resize: null,
  edits: { ...NO_EDITS },
  annotations: [],
  redactions: [],
  padding: 0,
  flatten: false,
  optimize: false,
});

export const initialState: State = {
  phase: "empty",
  tray: [],
  active: 0,
  operations: null,
  operation: null,
  target: null,
  options: defaultOptions(),
  metadata: { ...EMPTY_METADATA_EDIT },
  maxDimension: null,
  output: null,
  job: null,
  outcome: null,
  error: null,
  view: "radial",
  notice: null,
};

export function activeFile(s: State): FileInfo | null {
  const item = s.tray[s.active];
  return item && item.type === "file" ? item : null;
}

export function activeOperation(s: State): Operation | null {
  return s.operations?.find((o) => o.id === s.operation) ?? null;
}

/** The conversion capability for Convert with a chosen target. */
export function activeCapability(s: State) {
  return activeOperation(s)?.targets.find((c) => c.to === s.target) ?? null;
}

/** Format of the result, once it is known. */
export function outputFormat(s: State): Format | null {
  if (s.operation === "convert") return s.target;
  return activeOperation(s)?.outputFormat ?? null;
}

/** Whether everything needed to run is in place. */
export function readyToRun(s: State): boolean {
  if (s.phase !== "configure" && s.phase !== "failed") return false;
  if (!s.operation || !s.output) return false;
  return s.operation !== "convert" || s.target !== null;
}

export function isUsable(item: ImportResult | undefined): boolean {
  return !!item && item.type === "file" && item.problem === null && item.format !== null;
}

function enterItem(s: State, tray: ImportResult[], index: number): State {
  const item = tray[index];
  return {
    ...initialState,
    tray,
    active: index,
    notice: s.notice,
    phase: isUsable(item) ? "choose" : "rejected",
  };
}

/** Options when an operation is picked. Edits, shapes and redactions belong
 *  to the file and survive switching operations. */
function optionsFor(op: OperationId, settings: Settings, prev: ConvertOptions, format: Format | null): ConvertOptions {
  const quality =
    op === "compress"
      ? null // the operation's own default (75) applies
      : format === "jpeg"
        ? settings.jpegQuality
        : format === "webp"
          ? settings.webpQuality
          : null;
  return {
    ...prev,
    quality,
    lossless: false,
    background: settings.background,
    padding: op === "background" ? prev.padding : 0,
    flatten: op === "background",
    optimize: op === "compress",
  };
}

const busy = (s: State) => s.phase === "converting";
const canPick = (s: State) =>
  !busy(s) && (s.phase === "choose" || s.phase === "configure" || s.phase === "done" || s.phase === "failed");

export function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "imported": {
      if (busy(s) || a.items.length === 0) return s;
      const first = a.items.findIndex(isUsable);
      const skipped = a.items.length - a.items.filter(isUsable).length;
      const notice =
        a.items.length > 1
          ? `${a.items.length} files added${skipped ? ` · ${skipped} can't be opened` : ""}. Work on them one at a time from the file list.`
          : null;
      return enterItem({ ...s, notice }, a.items, first === -1 ? 0 : first);
    }
    case "select_item":
      if (busy(s) || a.index < 0 || a.index >= s.tray.length) return s;
      return enterItem(s, s.tray, a.index);
    case "operations_loaded": {
      // A retried operation whose editor needs the preview opens it now.
      const op = a.operations.find((o) => o.id === s.operation);
      const view = s.phase === "configure" && op?.preview && s.view === "radial" ? "edit" : s.view;
      return { ...s, operations: a.operations, view };
    }
    case "operations_failed":
      return { ...s, operations: [], error: a.error };
    case "operation_chosen": {
      if (!canPick(s)) return s;
      const op = s.operations?.find((o) => o.id === a.operation);
      if (!op || !op.available) return s;
      if (s.operation === a.operation && s.phase === "configure") return s;
      const format = activeFile(s)?.format ?? null;
      return {
        ...s,
        phase: "configure",
        operation: a.operation,
        target: null,
        options: optionsFor(a.operation, a.settings, s.options, a.operation === "convert" ? null : format),
        maxDimension: null,
        output: null,
        outcome: null,
        error: null,
        view: op.preview ? "edit" : "radial",
      };
    }
    case "operation_cleared":
      if (busy(s) || (s.phase !== "configure" && s.phase !== "failed")) return s;
      return { ...s, phase: "choose", operation: null, target: null, output: null, error: null, view: "radial" };
    case "target_chosen": {
      if (!canPick(s) || s.operation !== "convert") return s;
      const cap = activeOperation(s)?.targets.find((c) => c.to === a.target);
      if (!cap || !cap.available) return s;
      if (s.target === a.target && s.phase === "configure") return s;
      return {
        ...s,
        phase: "configure",
        target: a.target,
        options: {
          ...s.options,
          quality: a.target === "jpeg" ? a.settings.jpegQuality : a.target === "webp" ? a.settings.webpQuality : null,
          lossless: false,
          background: a.settings.background,
        },
        output: null,
        outcome: null,
        error: null,
        view: s.view === "result" ? "radial" : s.view,
      };
    }
    case "target_cleared":
      if (busy(s) || s.operation !== "convert" || s.target === null) return s;
      return { ...s, phase: "configure", target: null, output: null, view: "radial" };
    case "options_changed":
      return { ...s, options: { ...s.options, ...a.patch } };
    case "edits_changed":
      return { ...s, options: { ...s.options, edits: { ...s.options.edits, ...a.patch } } };
    case "edits_reset":
      return { ...s, options: { ...s.options, edits: { ...NO_EDITS } } };
    case "shapes_changed":
      return { ...s, options: { ...s.options, annotations: a.shapes } };
    case "regions_changed":
      return { ...s, options: { ...s.options, redactions: a.regions } };
    case "metadata_changed":
      return { ...s, metadata: { ...s.metadata, ...a.patch } };
    case "max_dimension_changed":
      return { ...s, maxDimension: a.value };
    case "output_suggested":
      return { ...s, output: { destDir: a.suggestion.destDir, stem: a.suggestion.stem } };
    case "output_changed":
      return s.output ? { ...s, output: { ...s.output, ...a.patch } } : s;
    case "view_changed":
      return { ...s, view: a.view };
    case "convert_started":
      if (!readyToRun(s)) return s;
      return { ...s, phase: "converting", job: { id: a.jobId, stage: "queued" }, error: null };
    case "job_stage":
      if (!s.job || s.job.id !== a.jobId) return s;
      return { ...s, job: { ...s.job, stage: a.stage } };
    case "convert_succeeded":
      if (!s.job || s.job.id !== a.jobId) return s;
      return { ...s, phase: "done", job: null, outcome: a.outcome, view: "result" };
    case "convert_failed":
      if (!s.job || s.job.id !== a.jobId) return s;
      return { ...s, phase: "failed", job: null, error: a.error };
    case "retry_loaded": {
      if (busy(s)) return s;
      const next = enterItem({ ...s, notice: null }, [a.item], 0);
      if (next.phase !== "choose") return next;
      // Operations are loaded asynchronously; keep the plan and let the
      // operation become active once they arrive (see `operations_loaded`).
      return {
        ...next,
        operation: a.operation,
        target: a.operation === "convert" ? a.target : null,
        options: { ...defaultOptions(), ...a.options, annotations: [], redactions: [] },
        output: a.output,
        phase: "configure",
      };
    }
    case "convert_another":
      if (s.phase !== "done" && s.phase !== "failed") return s;
      return {
        ...s,
        phase: "choose",
        operation: null,
        target: null,
        output: null,
        outcome: null,
        error: null,
        view: "radial",
      };
    case "dismiss_error":
      if (s.phase === "failed") return { ...s, phase: "configure", error: null };
      return { ...s, error: null };
    case "dismiss_notice":
      return { ...s, notice: null };
    case "reset":
      return busy(s) ? s : initialState;
  }
}
