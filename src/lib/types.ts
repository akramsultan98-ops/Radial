// Types mirroring the Rust core (`crates/radial-core`). Field names follow the
// serde `camelCase` renames used there.

export type Format = "png" | "jpeg" | "webp" | "bmp" | "tiff" | "gif";

export type ErrorKind =
  | "unsupported_format"
  | "unsupported_conversion"
  | "invalid_file"
  | "corrupt_file"
  | "empty_file"
  | "too_large"
  | "not_found"
  | "permission_denied"
  | "destination_exists"
  | "destination_not_writable"
  | "insufficient_storage"
  | "invalid_options"
  | "path_not_allowed"
  | "cancelled"
  | "engine_missing"
  | "engine_failed"
  | "io"
  | "internal";

export interface RadialError {
  kind: ErrorKind;
  message: string;
  retryable: boolean;
}

export type Recognized =
  | { kind: "convertible"; format: Format }
  | { kind: "known"; format: { id: string; label: string; category: string } };

export interface FileInfo {
  path: string;
  name: string;
  size: number;
  recognized: Recognized | null;
  /** Set only when the file is a convertible format whose header decoded. */
  format: Format | null;
  width: number | null;
  height: number | null;
  hasAlpha: boolean;
  bitDepth: number | null;
  extensionMismatch: boolean;
  problem: RadialError | null;
}

export type ImportResult =
  | ({ type: "file" } & FileInfo)
  | { type: "error"; name: string; error: RadialError };

export type OptionSpec =
  | { kind: "quality"; min: number; max: number; default: number }
  | { kind: "lossless" }
  | { kind: "background" }
  | { kind: "resize" }
  | { kind: "rotate" }
  | { kind: "crop" }
  | { kind: "brightness_contrast" };

export type EngineStatus = { state: "available" } | { state: "missing"; hint: string };

export interface Capability {
  from: Format;
  to: Format;
  toLabel: string;
  engine: string;
  engineLabel: string;
  engineStatus: EngineStatus;
  available: boolean;
  options: OptionSpec[];
  limitations: string[];
  progress: "stages";
  cancellation: "between_stages";
}

export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Edits {
  rotate: 0 | 90 | 180 | 270;
  crop: CropRect | null;
  brightness: number;
  contrast: number;
}

export interface Resize {
  width: number | null;
  height: number | null;
  keepAspect: boolean;
}

export interface ConvertOptions {
  quality: number | null;
  lossless: boolean;
  background: string | null;
  resize: Resize | null;
  edits: Edits;
}

export interface ConvertRequest {
  source: string;
  target: Format;
  destDir: string;
  stem: string;
  options: ConvertOptions;
}

export interface ConvertOutcome {
  outputPath: string;
  sourceFormat: Format;
  format: Format;
  width: number;
  height: number;
  sizeBytes: number;
}

export type JobStage = "queued" | "decoding" | "processing" | "encoding" | "verifying";

export interface OutputSuggestion {
  destDir: string;
  stem: string;
  fileName: string;
}

export interface HistoryEntry {
  id: string;
  sourceName: string;
  sourcePath: string;
  sourceFormat: Format;
  targetFormat: Format;
  timestamp: number;
  status: "succeeded" | "failed" | "cancelled";
  outputPath: string | null;
  errorKind: ErrorKind | null;
  errorMessage: string | null;
  destDir: string;
  stem: string;
  options: ConvertOptions;
}

export interface RetryPlan {
  file: ImportResult;
  target: Format;
  destDir: string;
  stem: string;
  options: ConvertOptions;
}

export interface Settings {
  defaultDestination: string | null;
  jpegQuality: number;
  webpQuality: number;
  background: string;
  keepHistory: boolean;
  motion: "system" | "reduced" | "full";
}

export const NO_EDITS: Edits = { rotate: 0, crop: null, brightness: 0, contrast: 0 };
