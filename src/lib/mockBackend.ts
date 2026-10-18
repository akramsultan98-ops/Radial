// In-browser stand-in for the native backend, compiled only with
// `vite --mode mock`. It drives UI tests and design review; it performs NO
// real conversion. Real conversions are verified by the Rust integration
// tests in crates/radial-core/tests.

import type { Backend, DragState } from "./backend";
import type {
  Capability,
  ConvertOutcome,
  ConvertRequest,
  Edits,
  FileInfo,
  Format,
  HistoryEntry,
  ImportResult,
  JobStage,
  OptionSpec,
  RadialError,
  Settings,
} from "./types";

const FORMATS: Format[] = ["png", "jpeg", "webp", "bmp", "tiff", "gif"];
const LABEL: Record<Format, string> = { png: "PNG", jpeg: "JPEG", webp: "WebP", bmp: "BMP", tiff: "TIFF", gif: "GIF" };
const EXT: Record<Format, string> = { png: "png", jpeg: "jpg", webp: "webp", bmp: "bmp", tiff: "tiff", gif: "gif" };
const ALPHA: Format[] = ["png", "webp", "tiff", "gif"];

function caps(from: Format): Capability[] {
  return FORMATS.filter((f) => f !== from).map((to) => {
    const options: OptionSpec[] = [];
    if (to === "jpeg") options.push({ kind: "quality", min: 1, max: 100, default: 90 });
    if (to === "webp") options.push({ kind: "quality", min: 1, max: 100, default: 85 }, { kind: "lossless" });
    if (!ALPHA.includes(to)) options.push({ kind: "background" });
    options.push({ kind: "resize" }, { kind: "rotate" }, { kind: "crop" }, { kind: "brightness_contrast" });
    return {
      from,
      to,
      toLabel: LABEL[to],
      engine: "native_image",
      engineLabel: "Built-in image engine",
      engineStatus: { state: "available" },
      available: true,
      options,
      limitations: ["Camera metadata (EXIF, GPS) is not copied; orientation is applied to the pixels."],
      progress: "stages",
      cancellation: "between_stages",
    };
  });
}

export function sampleFile(name = "Golden hour — 夕焼け.png", over: Partial<FileInfo> = {}): FileInfo {
  return {
    path: `/home/demo/Pictures/${name}`,
    name,
    size: 2_483_112,
    recognized: { kind: "convertible", format: "png" },
    format: "png",
    width: 2400,
    height: 1600,
    hasAlpha: true,
    bitDepth: 8,
    extensionMismatch: false,
    problem: null,
    ...over,
  };
}

const asImport = (f: FileInfo): ImportResult => ({ type: "file", ...f });

interface MockControls {
  nextPick: ImportResult[];
  /** Simulate a drop of these files. */
  drop(files: ImportResult[]): void;
  drag(state: DragState): void;
  /** Force the next conversion to fail with this error. */
  failNext: RadialError | null;
  /** Milliseconds per stage. */
  stageDelay: number;
  existing: Set<string>;
  sampleFile: typeof sampleFile;
}

declare global {
  interface Window {
    __radialMock?: MockControls;
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function renderSample(edits: Edits, maxDim: number, label: string): Promise<Blob> {
  const rotated = edits.rotate === 90 || edits.rotate === 270;
  let w = 1200;
  let h = 800;
  if (rotated) [w, h] = [h, w];
  const scale = Math.min(1, maxDim / Math.max(w, h));
  const canvas = document.createElement("canvas");
  const crop = edits.crop ?? { x: 0, y: 0, width: 1, height: 1 };
  canvas.width = Math.max(1, Math.round(w * crop.width * scale));
  canvas.height = Math.max(1, Math.round(h * crop.height * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  ctx.scale(scale, scale);
  ctx.translate(-crop.x * w, -crop.y * h);
  const g = ctx.createLinearGradient(0, 0, w, h);
  g.addColorStop(0, "#ff9a62");
  g.addColorStop(0.55, "#c2557a");
  g.addColorStop(1, "#2b2d63");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = "rgba(255,236,200,0.9)";
  ctx.beginPath();
  ctx.arc(w * 0.62, h * 0.55, Math.min(w, h) * 0.16, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#1b1530";
  ctx.beginPath();
  ctx.moveTo(0, h);
  ctx.lineTo(0, h * 0.72);
  ctx.lineTo(w * 0.3, h * 0.58);
  ctx.lineTo(w * 0.55, h * 0.74);
  ctx.lineTo(w * 0.8, h * 0.6);
  ctx.lineTo(w, h * 0.7);
  ctx.lineTo(w, h);
  ctx.fill();
  ctx.fillStyle = "rgba(255,255,255,0.85)";
  ctx.font = `600 ${Math.round(Math.min(w, h) * 0.05)}px sans-serif`;
  ctx.fillText(label, 40, 80);
  const b = 1 + edits.brightness / 200;
  const c = 1 + edits.contrast / 100;
  const out = document.createElement("canvas");
  out.width = canvas.width;
  out.height = canvas.height;
  const octx = out.getContext("2d");
  if (!octx) throw new Error("no canvas");
  octx.filter = `brightness(${b}) contrast(${c})`;
  octx.drawImage(canvas, 0, 0);
  return new Promise((resolve, reject) => out.toBlob((bl) => (bl ? resolve(bl) : reject(new Error("toBlob"))), "image/png"));
}

export function createMockBackend(): Backend {
  const stageListeners = new Set<(id: string, s: JobStage) => void>();
  const dropListeners = new Set<(f: ImportResult[]) => void>();
  const dragListeners = new Set<(s: DragState) => void>();
  const cancelled = new Set<string>();
  const outputs = new Set<string>();
  let history: HistoryEntry[] = [];
  let settings: Settings = {
    defaultDestination: null,
    jpegQuality: 90,
    webpQuality: 85,
    background: "#ffffff",
    keepHistory: true,
    motion: "system",
  };

  const controls: MockControls = {
    nextPick: [asImport(sampleFile())],
    drop: (files) => dropListeners.forEach((l) => l(files)),
    drag: (s) => dragListeners.forEach((l) => l(s)),
    failNext: null,
    stageDelay: 350,
    existing: new Set(),
    sampleFile,
  };
  window.__radialMock = controls;

  const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/"));
  const stemOf = (p: string) => {
    const n = p.slice(p.lastIndexOf("/") + 1);
    const dot = n.lastIndexOf(".");
    return dot > 0 ? n.slice(0, dot) : n;
  };
  const taken = (p: string) => controls.existing.has(p) || outputs.has(p);

  return {
    takeLaunchFiles: async () => [],
    pickFiles: async () => controls.nextPick,
    pickDirectory: async () => "/home/demo/Exports",
    sourceFormats: async () => FORMATS,
    capabilities: async (f) => caps(f),
    suggestOutput: async (source, target, destDir, stem) => {
      const dir = destDir ?? settings.defaultDestination ?? dirOf(source);
      const base = stem ?? stemOf(source);
      let s = base;
      for (let n = 2; taken(`${dir}/${s}.${EXT[target]}`); n++) s = `${base} (${n})`;
      return { destDir: dir, stem: s, fileName: `${s}.${EXT[target]}` };
    },
    convert: async (jobId: string, req: ConvertRequest): Promise<ConvertOutcome> => {
      const out = `${req.destDir}/${req.stem}.${EXT[req.target]}`;
      const record = (status: HistoryEntry["status"], err: RadialError | null, outputPath: string | null) => {
        if (!settings.keepHistory) return;
        history = [
          {
            id: jobId,
            sourceName: req.source.slice(req.source.lastIndexOf("/") + 1),
            sourcePath: req.source,
            sourceFormat: "png",
            targetFormat: req.target,
            timestamp: Date.now(),
            status,
            outputPath,
            errorKind: err?.kind ?? null,
            errorMessage: err?.message ?? null,
            destDir: req.destDir,
            stem: req.stem,
            options: req.options,
          },
          ...history,
        ];
      };
      const stages: JobStage[] = ["queued", "decoding", "processing", "encoding", "verifying"];
      for (const s of stages) {
        if (cancelled.has(jobId)) {
          const e: RadialError = { kind: "cancelled", message: "Conversion cancelled.", retryable: true };
          record("cancelled", e, null);
          throw e;
        }
        stageListeners.forEach((l) => l(jobId, s));
        await sleep(controls.stageDelay);
      }
      if (taken(out)) {
        const e: RadialError = {
          kind: "destination_exists",
          message: `“${req.stem}.${EXT[req.target]}” already exists. Choose another name.`,
          retryable: true,
        };
        record("failed", e, null);
        throw e;
      }
      if (controls.failNext) {
        const e = controls.failNext;
        controls.failNext = null;
        record("failed", e, null);
        throw e;
      }
      outputs.add(out);
      record("succeeded", null, out);
      const r = req.options.resize;
      return {
        outputPath: out,
        sourceFormat: "png",
        format: req.target,
        width: r?.width ?? 2400,
        height: r?.height ?? (r?.width ? Math.round((r.width * 2) / 3) : 1600),
        sizeBytes: req.target === "jpeg" ? 612_480 : 1_204_311,
      };
    },
    cancel: async (jobId) => {
      cancelled.add(jobId);
      return true;
    },
    preview: (path, edits, maxDim) =>
      renderSample(edits, maxDim, path.slice(path.lastIndexOf(".") + 1).toUpperCase()),
    history: async () => history,
    clearHistory: async () => {
      history = [];
    },
    retry: async (id) => {
      const e = history.find((h) => h.id === id);
      if (!e) throw { kind: "not_found", message: "That history entry no longer exists.", retryable: false };
      return {
        file: asImport(sampleFile(e.sourceName)),
        target: e.targetFormat,
        destDir: e.destDir,
        stem: e.stem,
        options: e.options,
      };
    },
    openPath: async () => {},
    revealPath: async () => {},
    getSettings: async () => settings,
    setSettings: async (s) => {
      settings = s;
      return s;
    },
    onJobStage: (cb) => {
      stageListeners.add(cb);
      return () => stageListeners.delete(cb);
    },
    onFilesDropped: (cb) => {
      dropListeners.add(cb);
      return () => dropListeners.delete(cb);
    },
    onDragState: (cb) => {
      dragListeners.add(cb);
      return () => dragListeners.delete(cb);
    },
  };
}
