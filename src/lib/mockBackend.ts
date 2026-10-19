// In-browser stand-in for the native backend, compiled only with
// `vite --mode mock`. It drives UI tests and design review; it performs NO
// real conversion. Real conversions are verified by the Rust integration
// tests in crates/radial-core/tests.

import type { Backend, DragState } from "./backend";
import type {
  Capability,
  Edits,
  FileInfo,
  Format,
  HistoryEntry,
  ImportResult,
  JobStage,
  MetadataReport,
  Operation,
  OperationId,
  OperationOutcome,
  OperationRequest,
  OptionSpec,
  RadialError,
  Settings,
} from "./types";

const FORMATS: Format[] = ["png", "jpeg", "webp", "bmp", "tiff", "gif"];
const LABEL: Record<Format, string> = {
  png: "PNG",
  jpeg: "JPEG",
  webp: "WebP",
  bmp: "BMP",
  tiff: "TIFF",
  gif: "GIF",
  pdf: "PDF",
  txt: "Text",
  docx: "Word",
};
const EXT: Record<Format, string> = {
  png: "png",
  jpeg: "jpg",
  webp: "webp",
  bmp: "bmp",
  tiff: "tiff",
  gif: "gif",
  pdf: "pdf",
  txt: "txt",
  docx: "docx",
};
const ALPHA: Format[] = ["png", "webp", "tiff", "gif"];

const cap = (from: Format, to: Format, engineLabel: string, options: OptionSpec[], limitations: string[]): Capability => ({
  from,
  to,
  toLabel: LABEL[to],
  engine: engineLabel === "PDF engine (PDFium)" ? "pdfium" : "pdf_writer",
  engineLabel,
  engineStatus: { state: "available" },
  available: true,
  options,
  limitations,
  progress: "stages",
  cancellation: "between_stages",
});

function pdfCaps(): Capability[] {
  const page: OptionSpec[] = [{ kind: "page" }, { kind: "dpi", min: 36, max: 600, default: 150 }];
  const note = ["Exports one page as a picture; its text is no longer selectable."];
  return [
    cap("pdf", "png", "PDF engine (PDFium)", page, note),
    cap("pdf", "jpeg", "PDF engine (PDFium)", [...page, { kind: "quality", min: 1, max: 100, default: 90 }], note),
    cap("pdf", "txt", "PDF engine (PDFium)", [], ["Scanned pages have no text layer and produce nothing (no OCR)."]),
  ];
}

function caps(from: Format): Capability[] {
  if (from === "pdf") return pdfCaps();
  if (from === "txt") return [cap("txt", "pdf", "PDF engine (PDFium)", [], ["Typeset on A4 in a single font and size."])];
  if (from === "docx") {
    return [
      {
        ...cap("docx", "txt", "Built-in Word reader", [], ["Keeps the words and paragraphs; formatting and images are left out."]),
        engine: "docx",
      },
    ];
  }
  const toPdf = cap(from, "pdf", "Built-in PDF writer", [], ["The image is placed on an A4 page."]);
  return [...FORMATS.filter((f) => f !== from).map((to) => {
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
    } satisfies Capability;
  }), toPdf];
}

const SUFFIX: Record<OperationId, string> = {
  convert: "",
  compress: "-compressed",
  edit: "-edited",
  crop: "-cropped",
  annotate: "-annotated",
  redact: "-redacted",
  background: "-background",
  metadata: "-metadata",
};

/** Mirrors `pdf_operations` in the Rust core. */
function pdfOperations(file: FileInfo): Operation[] {
  const op = (id: OperationId, label: string, description: string, options: OptionSpec[], extra: Partial<Operation> = {}): Operation => ({
    id,
    label,
    description,
    available: !file.restricted || id === "metadata",
    reason: file.restricted && id !== "metadata" ? "This PDF has security restrictions (an owner password), so RADIAL won't change or export it." : null,
    engineLabel: "PDF engine (PDFium)",
    options,
    limitations: [],
    outputFormat: "pdf",
    targets: [],
    preview: false,
    cancellation: "between_stages",
    ...extra,
  });
  return [
    op("convert", "Convert", "Export pages as images or text", [], { outputFormat: null, targets: pdfCaps() }),
    op("compress", "Compress", "Make the PDF smaller", [{ kind: "quality", min: 10, max: 95, default: 60 }, { kind: "max_dimension" }], {
      limitations: ["If the result isn't smaller than the original, nothing is saved."],
    }),
    op("edit", "Edit", "Change text and layout", [], {
      available: false,
      reason: "Editing PDF text and layout isn't supported yet. Use Annotate to add text or Crop to trim pages.",
    }),
    op("crop", "Crop", "Trim page margins", [{ kind: "page" }, { kind: "crop" }, { kind: "page_scope" }], { preview: true }),
    op("annotate", "Annotate", "Draw arrows, shapes and text on pages", [{ kind: "page" }, { kind: "shapes" }], { preview: true }),
    op("redact", "Redact", "Permanently remove text and areas", [{ kind: "page" }, { kind: "regions" }, { kind: "terms" }], {
      preview: true,
      limitations: ["Each page with a redaction is replaced by an image of itself with the areas blacked out."],
    }),
    op("background", "Background", "Fill transparency or add a border", [], {
      available: false,
      reason: "Background applies to images. Pages in a PDF already have a background.",
    }),
    op("metadata", "Metadata", "View, edit or remove document properties", file.restricted ? [] : [{ kind: "metadata_fields" }, { kind: "strip_metadata" }]),
  ];
}

/** Mirrors `document_operations` in the Rust core. */
function documentOperations(f: "txt" | "docx"): Operation[] {
  const ids: OperationId[] = ["convert", "compress", "edit", "crop", "annotate", "redact", "background", "metadata"];
  return ids.map((id) => {
    const live = id === "convert" || (id === "metadata" && f === "docx");
    const label = id[0]?.toUpperCase() + id.slice(1);
    return {
      id,
      label,
      description: id === "convert" ? (f === "docx" ? "Extract the text" : "Make a PDF") : id === "metadata" ? "View, edit or remove document properties" : label,
      available: live,
      reason: live ? null : `${label} isn't available for ${f === "docx" ? "Word documents" : "text files"} yet. It works on images and PDFs.`,
      engineLabel: f === "docx" ? "Built-in Word reader" : "Built-in text engine",
      options: id === "metadata" && live ? [{ kind: "metadata_fields" }, { kind: "strip_metadata" }] : [],
      limitations: [],
      outputFormat: id === "convert" ? null : f,
      targets: id === "convert" ? caps(f) : [],
      preview: false,
      cancellation: "between_stages",
    };
  });
}

/** Mirrors `ops::operations_for` in the Rust core for image files. */
function operations(file: FileInfo): Operation[] {
  if (file.format === "pdf") return pdfOperations(file);
  if (file.format === "txt" || file.format === "docx") return documentOperations(file.format);
  const f = file.format ?? "png";
  const op = (id: OperationId, label: string, description: string, options: OptionSpec[], extra: Partial<Operation> = {}): Operation => ({
    id,
    label,
    description,
    available: true,
    reason: null,
    engineLabel: "Built-in image engine",
    options,
    limitations: ["Camera metadata (EXIF, GPS) is not copied to the result."],
    outputFormat: f,
    targets: [],
    preview: false,
    cancellation: "between_stages",
    ...extra,
  });
  const quality: OptionSpec[] = f === "jpeg" || f === "webp" ? [{ kind: "quality", min: 1, max: 100, default: 75 }] : [];
  const compressible = f === "jpeg" || f === "webp" || f === "png" || f === "tiff";
  return [
    op("convert", "Convert", "Save in another format", [], { outputFormat: null, targets: caps(f) }),
    op("compress", "Compress", "Make the file smaller", [...quality, ...(f === "png" ? [{ kind: "optimize" } as const] : []), { kind: "max_dimension" }], {
      available: compressible,
      reason: compressible ? null : f === "bmp" ? "BMP is uncompressed by design. Convert to PNG for a lossless smaller file." : "GIF compression would drop animation frames. Convert to PNG or WebP instead.",
      limitations: ["If the result isn't smaller than the original, nothing is saved."],
    }),
    op("edit", "Edit", "Rotate, resize and adjust tone", [{ kind: "rotate" }, { kind: "resize" }, { kind: "brightness_contrast" }, ...quality], { preview: true }),
    op("crop", "Crop", "Trim to an area", [{ kind: "crop" }, { kind: "rotate" }], { preview: true }),
    op("annotate", "Annotate", "Draw arrows, shapes and text", [{ kind: "shapes" }], {
      preview: true,
      limitations: ["Annotations become part of the image pixels in the saved copy."],
    }),
    op("redact", "Redact", "Black out sensitive areas", [{ kind: "regions" }], {
      preview: true,
      limitations: ["The pixels under each box are replaced with solid black in the saved copy; nothing underneath can be recovered from it.", "All metadata, including location and embedded thumbnails, is removed."],
    }),
    op("background", "Background", file.hasAlpha ? "Fill transparency with a colour" : "Add a coloured border", [{ kind: "background" }, { kind: "padding" }], { preview: true }),
    op("metadata", "Metadata", "View, edit or remove metadata", f === "jpeg" || f === "png" ? [{ kind: "metadata_fields" }, { kind: "strip_metadata" }] : [], {
      limitations: [f === "jpeg" || f === "png" ? "Edits are lossless: image data is copied unchanged." : `${LABEL[f]} metadata can be viewed but not edited yet.`],
    }),
  ];
}

function metadataReport(file: FileInfo): MetadataReport {
  if (file.format === "docx") {
    return {
      fields: [
        { key: "title", label: "Title", value: "Client memo", editable: true },
        { key: "author", label: "Author", value: "Old author", editable: true },
        { key: "subject", label: "Subject", value: null, editable: true },
        { key: "keywords", label: "Keywords", value: null, editable: true },
      ],
      details: [
        { label: "Last modified by", value: "Jane Doe" },
        { label: "Company", value: "ACME Corporation" },
      ],
      canEdit: true,
      canStrip: true,
      hasLocation: false,
      notes: ["“Last modified by”, company and dates are read-only here; “Remove all” clears them."],
    };
  }
  if (file.format === "pdf") {
    const editable = !file.restricted && !file.signed;
    return {
      fields: [
        { key: "title", label: "Title", value: "Quarterly report", editable },
        { key: "author", label: "Author", value: "Account team", editable },
        { key: "subject", label: "Subject", value: null, editable },
        { key: "keywords", label: "Keywords", value: null, editable },
      ],
      details: [
        { label: "Pages", value: String(file.pages ?? 1) },
        { label: "PDF producer", value: "ReportLab" },
      ],
      canEdit: editable,
      canStrip: editable,
      hasLocation: false,
      notes: ["PDFs have no standard copyright field. Created/modified dates and the producer are shown but not edited."],
    };
  }
  const editable = file.format === "jpeg" || file.format === "png";
  return {
    fields: [
      { key: "title", label: "Title", value: "Golden hour", editable },
      { key: "author", label: "Author", value: null, editable },
      { key: "subject", label: "Subject", value: null, editable },
      { key: "keywords", label: "Keywords", value: null, editable },
      { key: "copyright", label: "Copyright", value: null, editable },
    ],
    details: [
      { label: "Camera model", value: "X1" },
      { label: "GPS location", value: "30° 2' 40\" N 31° 14' 9\" E" },
    ],
    canEdit: editable,
    canStrip: editable,
    hasLocation: true,
    notes: editable ? [] : [`Editing metadata isn't supported for ${LABEL[file.format ?? "png"]} yet.`],
  };
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
    pages: null,
    pageSizes: [],
    restricted: false,
    signed: false,
    extensionMismatch: false,
    problem: null,
    ...over,
  };
}

export function sampleDocx(name = "Client memo.docx", over: Partial<FileInfo> = {}): FileInfo {
  return sampleFile(name, {
    path: `/home/demo/Documents/${name}`,
    size: 36_920,
    recognized: { kind: "convertible", format: "docx" },
    format: "docx",
    width: null,
    height: null,
    hasAlpha: false,
    bitDepth: null,
    ...over,
  });
}

export function samplePdf(name = "Quarterly report.pdf", over: Partial<FileInfo> = {}): FileInfo {
  return sampleFile(name, {
    path: `/home/demo/Documents/${name}`,
    size: 842_113,
    recognized: { kind: "convertible", format: "pdf" },
    format: "pdf",
    width: 595,
    height: 842,
    hasAlpha: false,
    bitDepth: null,
    pages: 3,
    pageSizes: [
      [595, 842],
      [842, 595],
      [595, 842],
    ],
    ...over,
  });
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
  samplePdf: typeof samplePdf;
  sampleDocx: typeof sampleDocx;
  /** Pages requested from `preview`, most recent last. */
  previewPages: number[];
}

declare global {
  interface Window {
    __radialMock?: MockControls;
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A white page with a few grey "lines of text", sized like the PDF page. */
async function renderPage(file: FileInfo, page: number, maxDim: number): Promise<Blob> {
  const [pw, ph] = file.pageSizes[page] ?? [595, 842];
  const scale = maxDim / Math.max(pw, ph);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(pw * scale));
  canvas.height = Math.max(1, Math.round(ph * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  ctx.scale(scale, scale);
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, pw, ph);
  ctx.fillStyle = "#1d1d1f";
  ctx.font = "600 22px sans-serif";
  ctx.fillText(`Page ${page + 1}`, 60, 80);
  ctx.fillStyle = "#9aa0a6";
  for (let i = 0; i < 12; i++) ctx.fillRect(60, 120 + i * 28, (pw - 120) * (0.55 + ((i * 37) % 40) / 100), 10);
  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("toBlob"))), "image/png"));
}

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
  const known = new Map<string, FileInfo>();
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
    drop: (files) => dropListeners.forEach((l) => l(remember(files))),
    drag: (s) => dragListeners.forEach((l) => l(s)),
    failNext: null,
    stageDelay: 350,
    existing: new Set(),
    sampleFile,
    samplePdf,
    sampleDocx,
    previewPages: [],
  };
  window.__radialMock = controls;

  function remember(items: ImportResult[]): ImportResult[] {
    for (const i of items) if (i.type === "file") known.set(i.path, i);
    return items;
  }
  const fileFor = (path: string): FileInfo => known.get(path) ?? sampleFile();

  const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/"));
  const stemOf = (p: string) => {
    const n = p.slice(p.lastIndexOf("/") + 1);
    const dot = n.lastIndexOf(".");
    return dot > 0 ? n.slice(0, dot) : n;
  };
  const taken = (p: string) => controls.existing.has(p) || outputs.has(p);

  return {
    takeLaunchFiles: async () => [],
    pickFiles: async () => remember(controls.nextPick),
    pickDirectory: async () => "/home/demo/Exports",
    sourceFormats: async () => [...FORMATS, "pdf", "txt", "docx"],
    operations: async (path) => operations(fileFor(path)),
    readMetadata: async (path) => metadataReport(fileFor(path)),
    suggestOutput: async (source, operation, target, destDir, stem) => {
      const dir = destDir ?? settings.defaultDestination ?? dirOf(source);
      const base = stem ?? `${stemOf(source)}${SUFFIX[operation]}`;
      let s = base;
      for (let n = 2; taken(`${dir}/${s}.${EXT[target]}`); n++) s = `${base} (${n})`;
      return { destDir: dir, stem: s, fileName: `${s}.${EXT[target]}` };
    },
    runOperation: async (jobId: string, req: OperationRequest): Promise<OperationOutcome> => {
      const file = fileFor(req.source);
      const target: Format = req.target ?? file.format ?? "png";
      const out = `${req.destDir}/${req.stem}.${EXT[target]}`;
      const record = (status: HistoryEntry["status"], err: RadialError | null, outputPath: string | null) => {
        if (!settings.keepHistory) return;
        history = [
          {
            id: jobId,
            sourceName: req.source.slice(req.source.lastIndexOf("/") + 1),
            sourcePath: req.source,
            sourceFormat: file.format ?? "png",
            operation: req.operation,
            targetFormat: target,
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
          message: `“${req.stem}.${EXT[target]}” already exists. Choose another name.`,
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
      const size = req.operation === "compress" ? 912_400 : target === "jpeg" ? 612_480 : 1_204_311;
      if (file.format === "docx" || file.format === "txt") {
        return {
          outputPath: out,
          operation: req.operation,
          sourceFormat: file.format,
          format: target,
          width: null,
          height: null,
          sizeBytes: target === "txt" ? 1_204 : 36_410,
          sourceSizeBytes: file.size,
          notes: [],
        };
      }
      if (file.format === "pdf" || target === "pdf") {
        const pdfOut = target === "pdf";
        const notes =
          req.operation === "redact"
            ? [
                `${req.pdf.regions.length} area${req.pdf.regions.length === 1 ? "" : "s"} removed; those pages are now images.`,
                ...(req.pdf.terms.length ? [`Search found ${req.pdf.terms.length} match${req.pdf.terms.length === 1 ? "" : "es"}.`] : []),
              ]
            : [];
        return {
          outputPath: out,
          operation: req.operation,
          sourceFormat: file.format ?? "pdf",
          format: target,
          width: target === "txt" ? null : pdfOut ? 595 : 1240,
          height: target === "txt" ? null : pdfOut ? 842 : 1754,
          sizeBytes: target === "txt" ? 4_212 : size,
          sourceSizeBytes: file.size,
          notes,
        };
      }
      return {
        outputPath: out,
        operation: req.operation,
        sourceFormat: file.format ?? "png",
        format: target,
        width: r?.width ?? 2400,
        height: r?.height ?? (r?.width ? Math.round((r.width * 2) / 3) : 1600),
        sizeBytes: size,
        sourceSizeBytes: file.size,
        notes:
          req.operation === "redact"
            ? [`${req.options.redactions.length} area${req.options.redactions.length === 1 ? "" : "s"} replaced with black pixels; metadata removed.`]
            : [],
      };
    },
    cancel: async (jobId) => {
      cancelled.add(jobId);
      return true;
    },
    preview: (path, edits, maxDim, page = 0) => {
      const f = known.get(path);
      if (f?.format === "pdf" || path.endsWith(".pdf")) {
        controls.previewPages.push(page);
        return renderPage(f ?? samplePdf(), page, maxDim);
      }
      return renderSample(edits, maxDim, path.slice(path.lastIndexOf(".") + 1).toUpperCase());
    },
    history: async () => history,
    clearHistory: async () => {
      history = [];
    },
    retry: async (id) => {
      const e = history.find((h) => h.id === id);
      if (!e) throw { kind: "not_found", message: "That history entry no longer exists.", retryable: false };
      const file = asImport(sampleFile(e.sourceName));
      remember([file]);
      return {
        file,
        operation: e.operation,
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
