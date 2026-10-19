import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Backend, DragState, Unsubscribe } from "./backend";
import type { ImportResult, JobStage } from "./types";

/** Wrap an async `listen` so callers get a synchronous unsubscribe. */
function subscribe(start: () => Promise<() => void>): Unsubscribe {
  let stop: (() => void) | null = null;
  let cancelled = false;
  void start().then((fn) => {
    if (cancelled) fn();
    else stop = fn;
  });
  return () => {
    cancelled = true;
    stop?.();
  };
}

export function createTauriBackend(): Backend {
  return {
    takeLaunchFiles: () => invoke("take_launch_files"),
    pickFiles: () => invoke("pick_files"),
    pickDirectory: () => invoke("pick_directory"),
    sourceFormats: () => invoke("source_formats"),
    operations: (path) => invoke("operations_for", { path }),
    readMetadata: (path) => invoke("read_metadata", { path }),
    suggestOutput: (source, operation, target, destDir, stem) =>
      invoke("suggest_output", { source, operation, target, destDir: destDir ?? null, stem: stem ?? null }),
    runOperation: (jobId, request) => invoke("run_operation", { jobId, request }),
    cancel: (jobId) => invoke("cancel_job", { jobId }),
    preview: async (path, edits, maxDim, page) => {
      const bytes = await invoke<ArrayBuffer>("preview", { path, edits, maxDim, page: page ?? null });
      return new Blob([bytes]);
    },
    history: () => invoke("history_list"),
    clearHistory: () => invoke("history_clear"),
    retry: (id) => invoke("history_retry", { id }),
    openPath: (path) => invoke("open_path", { path }),
    revealPath: (path) => invoke("reveal_path", { path }),
    getSettings: () => invoke("get_settings"),
    setSettings: (settings) => invoke("set_settings", { settings }),
    onJobStage: (cb) =>
      subscribe(() =>
        listen<{ jobId: string; stage: JobStage }>("radial://job", (e) => cb(e.payload.jobId, e.payload.stage)),
      ),
    onFilesDropped: (cb) => subscribe(() => listen<ImportResult[]>("radial://files-dropped", (e) => cb(e.payload))),
    onDragState: (cb: (s: DragState) => void) =>
      subscribe(() =>
        getCurrentWebview().onDragDropEvent((e) => {
          const t = e.payload.type;
          cb(t === "enter" || t === "over" ? "over" : t === "drop" ? "drop" : "leave");
        }),
      ),
  };
}
