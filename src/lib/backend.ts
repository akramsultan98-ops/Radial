// The boundary between the UI and native code. Components talk to a
// `Backend`; in the desktop app that is Tauri IPC, in UI tests a mock.

import type {
  Edits,
  Format,
  HistoryEntry,
  ImportResult,
  JobStage,
  MetadataReport,
  Operation,
  OperationId,
  OperationOutcome,
  OperationRequest,
  OutputSuggestion,
  RadialError,
  RetryPlan,
  Settings,
} from "./types";

export type Unsubscribe = () => void;

export type DragState = "over" | "leave" | "drop";

export interface Backend {
  takeLaunchFiles(): Promise<ImportResult[]>;
  pickFiles(): Promise<ImportResult[]>;
  pickDirectory(): Promise<string | null>;
  sourceFormats(): Promise<Format[]>;
  /** Operations the registry offers for a file the user opened. */
  operations(path: string): Promise<Operation[]>;
  readMetadata(path: string): Promise<MetadataReport>;
  suggestOutput(
    source: string,
    operation: OperationId,
    target: Format,
    destDir?: string,
    stem?: string,
  ): Promise<OutputSuggestion>;
  runOperation(jobId: string, request: OperationRequest): Promise<OperationOutcome>;
  cancel(jobId: string): Promise<boolean>;
  /** Rendered preview bytes (JPEG or PNG). */
  preview(path: string, edits: Edits, maxDim: number): Promise<Blob>;
  history(): Promise<HistoryEntry[]>;
  clearHistory(): Promise<void>;
  retry(id: string): Promise<RetryPlan>;
  openPath(path: string): Promise<void>;
  revealPath(path: string): Promise<void>;
  getSettings(): Promise<Settings>;
  setSettings(settings: Settings): Promise<Settings>;
  onJobStage(cb: (jobId: string, stage: JobStage) => void): Unsubscribe;
  onFilesDropped(cb: (files: ImportResult[]) => void): Unsubscribe;
  onDragState(cb: (state: DragState) => void): Unsubscribe;
}

/** Normalise anything thrown across IPC into a RadialError. */
export function toRadialError(e: unknown): RadialError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    const r = e as RadialError;
    return { kind: r.kind, message: r.message, retryable: Boolean(r.retryable) };
  }
  return { kind: "internal", message: e instanceof Error ? e.message : String(e), retryable: false };
}

let instance: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  instance ??= __RADIAL_MOCK__
    ? import("./mockBackend").then((m) => m.createMockBackend())
    : import("./tauriBackend").then((m) => m.createTauriBackend());
  return instance;
}
