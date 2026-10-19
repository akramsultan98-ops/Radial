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
  | "no_reduction"
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
  | { kind: "brightness_contrast" }
  | { kind: "max_dimension" }
  | { kind: "padding" }
  | { kind: "optimize" }
  | { kind: "shapes" }
  | { kind: "regions" }
  | { kind: "metadata_fields" }
  | { kind: "strip_metadata" };

export type OperationId =
  | "convert"
  | "compress"
  | "edit"
  | "crop"
  | "annotate"
  | "redact"
  | "background"
  | "metadata";

export interface Operation {
  id: OperationId;
  label: string;
  description: string;
  available: boolean;
  reason: string | null;
  engineLabel: string;
  options: OptionSpec[];
  limitations: string[];
  /** Format of the result; null when the user chooses (Convert). */
  outputFormat: Format | null;
  targets: Capability[];
  preview: boolean;
  cancellation: "between_stages";
}

export type ShapeKind = "rect" | "ellipse" | "line" | "arrow" | "text";

/** Coordinates are fractions (0–1) of the edited image; stroke and size are
 *  fractions of its shorter side. */
export interface Shape {
  kind: ShapeKind;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
  stroke: number;
  filled: boolean;
  text: string;
  size: number;
}

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
  annotations: Shape[];
  redactions: CropRect[];
  padding: number;
  flatten: boolean;
  optimize: boolean;
}

export interface MetadataEdit {
  /** Empty strings remove a field. */
  fields: Record<string, string>;
  stripAll: boolean;
}

export interface OperationRequest {
  source: string;
  operation: OperationId;
  target: Format | null;
  destDir: string;
  stem: string;
  options: ConvertOptions;
  metadata: MetadataEdit;
  maxDimension: number | null;
}

export interface OperationOutcome {
  outputPath: string;
  operation: OperationId;
  sourceFormat: Format;
  format: Format;
  width: number | null;
  height: number | null;
  sizeBytes: number;
  sourceSizeBytes: number;
  notes: string[];
}

export interface MetaField {
  key: string;
  label: string;
  value: string | null;
  editable: boolean;
}

export interface MetadataReport {
  fields: MetaField[];
  details: { label: string; value: string }[];
  canEdit: boolean;
  canStrip: boolean;
  hasLocation: boolean;
  notes: string[];
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
  operation: OperationId;
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
  operation: OperationId;
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

export const EMPTY_METADATA_EDIT: MetadataEdit = { fields: {}, stripAll: false };
