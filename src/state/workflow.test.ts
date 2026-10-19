import { describe, expect, it } from "vitest";
import { sampleFile } from "../lib/mockBackend";
import type { Capability, ImportResult, Operation, OperationId, OperationOutcome, Settings } from "../lib/types";
import {
  activeFile,
  defaultOptions,
  initialState,
  outputFormat,
  readyToRun,
  reducer,
  type Action,
  type State,
} from "./workflow";

const settings: Settings = {
  defaultDestination: null,
  jpegQuality: 77,
  webpQuality: 66,
  background: "#000000",
  keepHistory: true,
  motion: "system",
};

const file = (over = {}): ImportResult => ({ type: "file", ...sampleFile("a.png", over) });
const cap = (to: Capability["to"], available = true): Capability => ({
  from: "png",
  to,
  toLabel: to.toUpperCase(),
  engine: "native_image",
  engineLabel: "Built-in",
  engineStatus: available ? { state: "available" } : { state: "missing", hint: "install" },
  available,
  options: [],
  limitations: [],
  progress: "stages",
  cancellation: "between_stages",
});

const op = (id: OperationId, extra: Partial<Operation> = {}): Operation => ({
  id,
  label: id,
  description: id,
  available: true,
  reason: null,
  engineLabel: "Built-in",
  options: [],
  limitations: [],
  outputFormat: id === "convert" ? null : "png",
  targets: id === "convert" ? [cap("jpeg"), cap("webp"), cap("gif", false)] : [],
  preview: ["edit", "crop", "annotate", "redact", "background"].includes(id),
  cancellation: "between_stages",
  ...extra,
});

const OPS = [op("convert"), op("compress"), op("redact"), op("metadata"), op("background", { available: false, reason: "no" })];

const run = (actions: Action[], from: State = initialState) => actions.reduce(reducer, from);

const ready = () =>
  run([
    { type: "imported", items: [file()] },
    { type: "operations_loaded", operations: OPS },
  ]);

const outcome = (over: Partial<OperationOutcome> = {}): OperationOutcome => ({
  outputPath: "/out/a.jpg",
  operation: "convert",
  sourceFormat: "png",
  format: "jpeg",
  width: 1,
  height: 1,
  sizeBytes: 1,
  sourceSizeBytes: 2,
  notes: [],
  ...over,
});

describe("workflow reducer", () => {
  it("starts empty", () => {
    expect(initialState.phase).toBe("empty");
    expect(activeFile(initialState)).toBeNull();
  });

  it("moves to choose when a usable file is imported", () => {
    const s = ready();
    expect(s.phase).toBe("choose");
    expect(activeFile(s)?.name).toBe("a.png");
    expect(s.operations).toHaveLength(OPS.length);
  });

  it("parks unusable files in rejected", () => {
    const bad = file({ format: null, problem: { kind: "corrupt_file", message: "bad", retryable: false } });
    expect(run([{ type: "imported", items: [bad] }]).phase).toBe("rejected");
    const err: ImportResult = { type: "error", name: "x", error: { kind: "not_found", message: "", retryable: false } };
    expect(run([{ type: "imported", items: [err] }]).phase).toBe("rejected");
  });

  it("picks the first usable file of a multi-file drop and explains", () => {
    const bad = file({ format: null, problem: { kind: "invalid_file", message: "?", retryable: false } });
    const s = run([{ type: "imported", items: [bad, file(), file()] }]);
    expect(s.active).toBe(1);
    expect(s.notice).toMatch(/3 files added · 1 can't be opened/);
  });

  it("chooses an operation and opens its editor when it previews", () => {
    const s = run([{ type: "operation_chosen", operation: "redact", settings }], ready());
    expect(s.phase).toBe("configure");
    expect(s.operation).toBe("redact");
    expect(s.view).toBe("edit");
    expect(outputFormat(s)).toBe("png");
    const m = run([{ type: "operation_chosen", operation: "metadata", settings }], ready());
    expect(m.view).toBe("radial");
  });

  it("refuses unavailable or unknown operations", () => {
    expect(run([{ type: "operation_chosen", operation: "background", settings }], ready()).phase).toBe("choose");
    expect(run([{ type: "operation_chosen", operation: "annotate", settings }], ready()).phase).toBe("choose");
  });

  it("requires a format for convert and applies per-format defaults", () => {
    let s = run([{ type: "operation_chosen", operation: "convert", settings }], ready());
    expect(outputFormat(s)).toBeNull();
    s = run([{ type: "output_suggested", suggestion: { destDir: "/o", stem: "a", fileName: "a" } }], s);
    expect(readyToRun(s)).toBe(false);
    s = run([{ type: "target_chosen", target: "jpeg", settings }], s);
    expect(s.options.quality).toBe(77);
    expect(s.options.background).toBe("#000000");
    expect(outputFormat(s)).toBe("jpeg");
    expect(run([{ type: "target_chosen", target: "gif", settings }], s).target).toBe("jpeg");
  });

  it("only accepts targets while converting", () => {
    const s = run([{ type: "operation_chosen", operation: "compress", settings }], ready());
    expect(run([{ type: "target_chosen", target: "jpeg", settings }], s).target).toBeNull();
  });

  it("keeps edits, shapes and redactions when switching operation", () => {
    const s = run(
      [
        { type: "operation_chosen", operation: "redact", settings },
        { type: "regions_changed", regions: [{ x: 0, y: 0, width: 0.5, height: 0.5 }] },
        { type: "edits_changed", patch: { rotate: 90 } },
        { type: "operation_cleared" },
        { type: "operation_chosen", operation: "compress", settings },
      ],
      ready(),
    );
    expect(s.options.redactions).toHaveLength(1);
    expect(s.options.edits.rotate).toBe(90);
    expect(s.options.optimize).toBe(true);
  });

  const configured = () =>
    run(
      [
        { type: "operation_chosen", operation: "convert", settings },
        { type: "target_chosen", target: "jpeg", settings },
        { type: "output_suggested", suggestion: { destDir: "/out", stem: "a", fileName: "a.jpg" } },
      ],
      ready(),
    );

  it("tracks a job through stages to success", () => {
    let s = run([{ type: "convert_started", jobId: "j1" }], configured());
    expect(s.phase).toBe("converting");
    expect(s.job?.stage).toBe("queued");
    s = run([{ type: "job_stage", jobId: "j1", stage: "encoding" }], s);
    expect(s.job?.stage).toBe("encoding");
    s = run([{ type: "job_stage", jobId: "other", stage: "verifying" }], s);
    expect(s.job?.stage).toBe("encoding");
    s = run([{ type: "convert_succeeded", jobId: "j1", outcome: outcome() }], s);
    expect(s.phase).toBe("done");
    expect(s.view).toBe("result");
    expect(s.outcome?.outputPath).toBe("/out/a.jpg");
  });

  it("ignores imports and resets while working", () => {
    const s = run([{ type: "convert_started", jobId: "j1" }], configured());
    expect(run([{ type: "imported", items: [file()] }], s)).toBe(s);
    expect(run([{ type: "reset" }], s)).toBe(s);
    expect(run([{ type: "operation_chosen", operation: "compress", settings }], s)).toBe(s);
  });

  it("recovers from failure back to configure", () => {
    let s = run(
      [
        { type: "convert_started", jobId: "j1" },
        { type: "convert_failed", jobId: "j1", error: { kind: "destination_exists", message: "x", retryable: true } },
      ],
      configured(),
    );
    expect(s.phase).toBe("failed");
    s = run([{ type: "convert_started", jobId: "j2" }], s);
    expect(s.phase).toBe("converting");
  });

  it("offers another action on the same file after success", () => {
    const s = run(
      [
        { type: "convert_started", jobId: "j" },
        { type: "convert_succeeded", jobId: "j", outcome: outcome() },
        { type: "convert_another" },
      ],
      configured(),
    );
    expect(s.phase).toBe("choose");
    expect(activeFile(s)?.name).toBe("a.png");
    expect(s.operation).toBeNull();
  });

  it("loads a retry plan without restoring sensitive content", () => {
    let s = run([
      {
        type: "retry_loaded",
        item: file(),
        operation: "redact",
        target: "png",
        output: { destDir: "/d", stem: "s" },
        options: { ...defaultOptions(), redactions: [{ x: 0, y: 0, width: 1, height: 1 }] },
      },
    ]);
    expect(s.phase).toBe("configure");
    expect(s.operation).toBe("redact");
    expect(s.target).toBeNull();
    expect(s.options.redactions).toEqual([]);
    s = run([{ type: "operations_loaded", operations: OPS }], s);
    expect(s.view).toBe("edit");
  });
});
