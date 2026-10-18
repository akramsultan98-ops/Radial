// The conversion workflow as a pure reducer:
//   empty → (file) → choose → (target) → configure → converting → done | failed
// A rejected file (unreadable/unsupported) parks in `rejected`.

import type {
  Capability,
  ConvertOptions,
  ConvertOutcome,
  Edits,
  FileInfo,
  Format,
  ImportResult,
  JobStage,
  OutputSuggestion,
  RadialError,
  Settings,
} from "../lib/types";
import { NO_EDITS } from "../lib/types";

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
  capabilities: Capability[] | null;
  target: Format | null;
  options: ConvertOptions;
  output: Output | null;
  job: { id: string; stage: JobStage } | null;
  outcome: ConvertOutcome | null;
  error: RadialError | null;
  view: StageView;
  /** A one-off message about the last import (e.g. skipped files). */
  notice: string | null;
}

export type Action =
  | { type: "imported"; items: ImportResult[] }
  | { type: "select_item"; index: number }
  | { type: "capabilities_loaded"; capabilities: Capability[] }
  | { type: "capabilities_failed"; error: RadialError }
  | { type: "target_chosen"; target: Format; settings: Settings }
  | { type: "target_cleared" }
  | { type: "options_changed"; patch: Partial<ConvertOptions> }
  | { type: "edits_changed"; patch: Partial<Edits> }
  | { type: "edits_reset" }
  | { type: "output_suggested"; suggestion: OutputSuggestion }
  | { type: "output_changed"; patch: Partial<Output> }
  | { type: "view_changed"; view: StageView }
  | { type: "convert_started"; jobId: string }
  | { type: "job_stage"; jobId: string; stage: JobStage }
  | { type: "convert_succeeded"; jobId: string; outcome: ConvertOutcome }
  | { type: "convert_failed"; jobId: string; error: RadialError }
  | { type: "retry_loaded"; item: ImportResult; target: Format; output: Output; options: ConvertOptions }
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
});

export const initialState: State = {
  phase: "empty",
  tray: [],
  active: 0,
  capabilities: null,
  target: null,
  options: defaultOptions(),
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

export function activeCapability(s: State): Capability | null {
  return s.capabilities?.find((c) => c.to === s.target) ?? null;
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

function optionsFor(target: Format, settings: Settings, prev: ConvertOptions): ConvertOptions {
  return {
    quality: target === "jpeg" ? settings.jpegQuality : target === "webp" ? settings.webpQuality : null,
    lossless: false,
    background: settings.background,
    resize: prev.resize,
    // Edits belong to the file, not the target, so they survive a format change.
    edits: prev.edits,
  };
}

const busy = (s: State) => s.phase === "converting";

export function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "imported": {
      if (busy(s) || a.items.length === 0) return s;
      const first = a.items.findIndex(isUsable);
      const skipped = a.items.length - a.items.filter(isUsable).length;
      const notice =
        a.items.length > 1
          ? `${a.items.length} files added${skipped ? ` · ${skipped} can't be converted` : ""}. Convert them one at a time from the file list.`
          : null;
      return enterItem({ ...s, notice }, a.items, first === -1 ? 0 : first);
    }
    case "select_item":
      if (busy(s) || a.index < 0 || a.index >= s.tray.length) return s;
      return enterItem(s, s.tray, a.index);
    case "capabilities_loaded":
      return { ...s, capabilities: a.capabilities };
    case "capabilities_failed":
      return { ...s, capabilities: [], error: a.error };
    case "target_chosen": {
      if (busy(s) || (s.phase !== "choose" && s.phase !== "configure" && s.phase !== "done" && s.phase !== "failed")) {
        return s;
      }
      const cap = s.capabilities?.find((c) => c.to === a.target);
      if (!cap || !cap.available) return s;
      const sameTarget = s.target === a.target && s.phase === "configure";
      return {
        ...s,
        phase: "configure",
        target: a.target,
        options: sameTarget ? s.options : optionsFor(a.target, a.settings, s.options),
        output: sameTarget ? s.output : null,
        outcome: null,
        error: null,
        view: s.view === "result" ? "radial" : s.view,
      };
    }
    case "target_cleared":
      if (busy(s) || s.phase !== "configure") return s;
      return { ...s, phase: "choose", target: null, output: null, view: "radial" };
    case "options_changed":
      return { ...s, options: { ...s.options, ...a.patch } };
    case "edits_changed":
      return { ...s, options: { ...s.options, edits: { ...s.options.edits, ...a.patch } } };
    case "edits_reset":
      return { ...s, options: { ...s.options, edits: { ...NO_EDITS } } };
    case "output_suggested":
      return { ...s, output: { destDir: a.suggestion.destDir, stem: a.suggestion.stem } };
    case "output_changed":
      return s.output ? { ...s, output: { ...s.output, ...a.patch } } : s;
    case "view_changed":
      return { ...s, view: a.view };
    case "convert_started":
      if (s.phase !== "configure" && s.phase !== "failed") return s;
      if (!s.target || !s.output) return s;
      return { ...s, phase: "converting", job: { id: a.jobId, stage: "queued" }, error: null, view: "radial" };
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
      return { ...next, target: a.target, options: a.options, output: a.output, phase: "configure" };
    }
    case "convert_another":
      if (s.phase !== "done" && s.phase !== "failed") return s;
      return { ...s, phase: "choose", target: null, output: null, outcome: null, error: null, view: "radial" };
    case "dismiss_error":
      if (s.phase === "failed") return { ...s, phase: "configure", error: null };
      return { ...s, error: null };
    case "dismiss_notice":
      return { ...s, notice: null };
    case "reset":
      return busy(s) ? s : initialState;
  }
}
