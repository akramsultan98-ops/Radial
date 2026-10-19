// Display helpers. Pure functions; unit tested.

import type { ErrorKind, Format, JobStage, OperationId } from "./types";

export const FORMAT_LABEL: Record<Format, string> = {
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

export const OPERATION_LABEL: Record<OperationId, string> = {
  convert: "Convert",
  compress: "Compress",
  edit: "Edit",
  crop: "Crop",
  annotate: "Annotate",
  redact: "Redact",
  background: "Background",
  metadata: "Metadata",
};

export const FORMAT_EXT: Record<Format, string> = {
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

/** One-line character of each format, shown in the accessible format list. */
export const FORMAT_HINT: Record<Format, string> = {
  png: "Lossless, transparent",
  jpeg: "Photos, small files",
  webp: "Web, transparent",
  bmp: "Uncompressed",
  tiff: "Archival, 16-bit",
  gif: "256 colours",
  pdf: "Documents, print",
  txt: "Plain text, no layout",
  docx: "Word document",
};

/** Formats the preview pipeline can render. */
export const PREVIEWABLE = (f: Format | null): boolean => f !== null && f !== "txt" && f !== "docx";

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1000) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1000;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatDims(w: number | null, h: number | null): string {
  return w && h ? `${w} × ${h}` : "—";
}

/** Relative change, e.g. "−42%" or "+8%". */
export function formatDelta(before: number, after: number): string {
  if (before <= 0) return "";
  const pct = Math.round(((after - before) / before) * 100);
  if (pct === 0) return "±0%";
  return pct < 0 ? `−${Math.abs(pct)}%` : `+${pct}%`;
}

export const STAGES: readonly JobStage[] = ["queued", "decoding", "processing", "encoding", "verifying"];

export const STAGE_LABEL: Record<JobStage, string> = {
  queued: "Waiting for a free slot",
  decoding: "Reading file",
  processing: "Applying changes",
  encoding: "Writing output",
  verifying: "Verifying result",
};

export function timeAgo(ts: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ts) / 1000));
  if (s < 45) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d < 7) return `${d} d ago`;
  return new Date(ts).toLocaleDateString();
}

export type Recovery = "rename" | "choose_folder" | "retry" | "choose_file" | "adjust_options" | "none";

/** The most useful next step for each error category. */
export function recoveryFor(kind: ErrorKind): Recovery {
  switch (kind) {
    case "destination_exists":
      return "rename";
    case "destination_not_writable":
    case "permission_denied":
    case "insufficient_storage":
      return "choose_folder";
    case "invalid_options":
    case "no_reduction":
      return "adjust_options";
    case "cancelled":
    case "io":
    case "engine_failed":
      return "retry";
    case "unsupported_format":
    case "unsupported_conversion":
    case "invalid_file":
    case "corrupt_file":
    case "empty_file":
    case "too_large":
    case "not_found":
    case "path_not_allowed":
      return "choose_file";
    default:
      return "none";
  }
}

/** Shorten a long path for display, keeping the end (the useful part). */
export function shortenPath(p: string, max = 42): string {
  if (p.length <= max) return p;
  return `…${p.slice(p.length - max + 1)}`;
}

let counter = 0;
export function newJobId(): string {
  counter += 1;
  const rand = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `job-${counter}-${rand}`;
}
