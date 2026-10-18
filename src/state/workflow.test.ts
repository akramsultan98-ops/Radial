import { describe, expect, it } from "vitest";
import { sampleFile } from "../lib/mockBackend";
import type { Capability, ImportResult, Settings } from "../lib/types";
import { activeFile, initialState, reducer, type Action, type State } from "./workflow";

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

const run = (actions: Action[], from: State = initialState) => actions.reduce(reducer, from);

const ready = () =>
  run([
    { type: "imported", items: [file()] },
    { type: "capabilities_loaded", capabilities: [cap("jpeg"), cap("webp"), cap("gif", false)] },
  ]);

describe("workflow reducer", () => {
  it("starts empty", () => {
    expect(initialState.phase).toBe("empty");
    expect(activeFile(initialState)).toBeNull();
  });

  it("moves to choose when a usable file is imported", () => {
    const s = ready();
    expect(s.phase).toBe("choose");
    expect(activeFile(s)?.name).toBe("a.png");
    expect(s.notice).toBeNull();
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
    expect(s.phase).toBe("choose");
    expect(s.notice).toMatch(/3 files added · 1 can't be converted/);
  });

  it("applies per-format defaults from settings when a target is chosen", () => {
    const s = run([{ type: "target_chosen", target: "jpeg", settings }], ready());
    expect(s.phase).toBe("configure");
    expect(s.options.quality).toBe(77);
    expect(s.options.background).toBe("#000000");
    const w = run([{ type: "target_chosen", target: "webp", settings }], s);
    expect(w.options.quality).toBe(66);
  });

  it("refuses unavailable or unknown targets", () => {
    expect(run([{ type: "target_chosen", target: "gif", settings }], ready()).phase).toBe("choose");
    expect(run([{ type: "target_chosen", target: "bmp", settings }], ready()).phase).toBe("choose");
  });

  it("keeps edits when switching target", () => {
    const s = run(
      [
        { type: "target_chosen", target: "jpeg", settings },
        { type: "edits_changed", patch: { rotate: 90, brightness: 20 } },
        { type: "target_chosen", target: "webp", settings },
      ],
      ready(),
    );
    expect(s.options.edits.rotate).toBe(90);
    expect(s.options.edits.brightness).toBe(20);
  });

  it("requires an output location before converting", () => {
    const s = run([{ type: "target_chosen", target: "jpeg", settings }, { type: "convert_started", jobId: "j" }], ready());
    expect(s.phase).toBe("configure");
  });

  const configured = () =>
    run(
      [
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
    // Events for other jobs are ignored.
    s = run([{ type: "job_stage", jobId: "other", stage: "verifying" }], s);
    expect(s.job?.stage).toBe("encoding");
    const outcome = { outputPath: "/out/a.jpg", sourceFormat: "png", format: "jpeg", width: 1, height: 1, sizeBytes: 1 } as const;
    s = run([{ type: "convert_succeeded", jobId: "j1", outcome }], s);
    expect(s.phase).toBe("done");
    expect(s.view).toBe("result");
    expect(s.outcome?.outputPath).toBe("/out/a.jpg");
  });

  it("ignores imports and resets while converting", () => {
    const s = run([{ type: "convert_started", jobId: "j1" }], configured());
    expect(run([{ type: "imported", items: [file()] }], s)).toBe(s);
    expect(run([{ type: "reset" }], s)).toBe(s);
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
    expect(s.error?.kind).toBe("destination_exists");
    s = run([{ type: "convert_started", jobId: "j2" }], s);
    expect(s.phase).toBe("converting");
  });

  it("offers another conversion of the same file after success", () => {
    const outcome = { outputPath: "/o", sourceFormat: "png", format: "jpeg", width: 1, height: 1, sizeBytes: 1 } as const;
    const s = run(
      [{ type: "convert_started", jobId: "j" }, { type: "convert_succeeded", jobId: "j", outcome }, { type: "convert_another" }],
      configured(),
    );
    expect(s.phase).toBe("choose");
    expect(activeFile(s)?.name).toBe("a.png");
    expect(s.target).toBeNull();
  });

  it("loads a retry plan straight into configure", () => {
    const s = run([
      {
        type: "retry_loaded",
        item: file(),
        target: "webp",
        output: { destDir: "/d", stem: "s" },
        options: { quality: 50, lossless: false, background: null, resize: null, edits: initialState.options.edits },
      },
    ]);
    expect(s.phase).toBe("configure");
    expect(s.target).toBe("webp");
    expect(s.output).toEqual({ destDir: "/d", stem: "s" });
  });
});
