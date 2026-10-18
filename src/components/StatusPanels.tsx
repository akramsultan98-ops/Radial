// Side-panel content for converting / done / failed.

import type { ReactNode } from "react";
import { FORMAT_LABEL, STAGES, STAGE_LABEL, formatBytes, formatDelta, recoveryFor, shortenPath } from "../lib/format";
import type { ConvertOutcome, FileInfo, JobStage, RadialError } from "../lib/types";
import { IconAlert, IconCheck, IconExternal, IconFolder, IconOpen, IconRetry } from "./Icons";

export function ProgressPanel({ stage, onCancel, cancelling }: { stage: JobStage; onCancel: () => void; cancelling: boolean }) {
  const current = STAGES.indexOf(stage);
  return (
    <section className="status" aria-labelledby="progress-title">
      <h2 id="progress-title" className="status__title">
        Converting…
      </h2>
      <ol className="stages" aria-live="polite">
        {STAGES.map((s, i) => (
          <li
            key={s}
            className={i < current ? "stages__item--done" : i === current ? "stages__item--active" : undefined}
            aria-current={i === current ? "step" : undefined}
          >
            <span className="stages__dot" aria-hidden="true">
              {i < current ? <IconCheck /> : null}
            </span>
            {STAGE_LABEL[s]}
          </li>
        ))}
      </ol>
      <p className="field__hint">Progress is reported by stage. Cancelling never leaves a partial file.</p>
      <button type="button" className="btn btn--block" onClick={onCancel} disabled={cancelling}>
        {cancelling ? "Cancelling…" : "Cancel"}
        <kbd>Esc</kbd>
      </button>
    </section>
  );
}

interface ResultProps {
  file: FileInfo;
  outcome: ConvertOutcome;
  onOpen: () => void;
  onReveal: () => void;
  onAnother: () => void;
  onNewFile: () => void;
}

export function ResultPanel({ file, outcome, onOpen, onReveal, onAnother, onNewFile }: ResultProps) {
  const name = outcome.outputPath.split(/[\\/]/).pop() ?? outcome.outputPath;
  const dir = outcome.outputPath.slice(0, outcome.outputPath.length - name.length - 1);
  return (
    <section className="status status--success" aria-labelledby="done-title">
      <div className="status__icon status__icon--success" aria-hidden="true">
        <IconCheck />
      </div>
      <h2 id="done-title" className="status__title" tabIndex={-1}>
        Converted to {FORMAT_LABEL[outcome.format]}
      </h2>
      <p className="status__file truncate selectable" title={outcome.outputPath}>
        {name}
      </p>
      <p className="card__path truncate mono selectable" title={dir}>
        {shortenPath(dir, 46)}
      </p>
      <dl className="facts">
        <div>
          <dt>Size</dt>
          <dd>
            {formatBytes(outcome.sizeBytes)} <span className="muted">{formatDelta(file.size, outcome.sizeBytes)}</span>
          </dd>
        </div>
        <div>
          <dt>Dimensions</dt>
          <dd>
            {outcome.width} × {outcome.height}
          </dd>
        </div>
      </dl>
      <div className="button-row">
        <button type="button" className="btn btn--primary" onClick={onOpen}>
          <IconExternal />
          Open
        </button>
        <button type="button" className="btn" onClick={onReveal}>
          <IconFolder />
          Show in folder
        </button>
      </div>
      <div className="button-row">
        <button type="button" className="btn btn--ghost" onClick={onAnother}>
          <IconRetry />
          Another format
        </button>
        <button type="button" className="btn btn--ghost" onClick={onNewFile}>
          <IconOpen />
          New file
        </button>
      </div>
    </section>
  );
}

const ERROR_TITLE: Partial<Record<RadialError["kind"], string>> = {
  destination_exists: "That name is taken",
  destination_not_writable: "Can't save there",
  permission_denied: "Permission denied",
  insufficient_storage: "Not enough disk space",
  cancelled: "Conversion cancelled",
  corrupt_file: "The file is damaged",
  too_large: "The image is too large",
  not_found: "The file was moved or deleted",
  engine_failed: "The conversion failed",
  invalid_options: "Check the options",
  path_not_allowed: "Open the file again",
};

interface ErrorProps {
  error: RadialError;
  onRename: () => void;
  onChooseFolder: () => void;
  onRetry: () => void;
  onChooseFile: () => void;
  onBack: () => void;
}

export function ErrorPanel({ error, onRename, onChooseFolder, onRetry, onChooseFile, onBack }: ErrorProps) {
  const recovery = recoveryFor(error.kind);
  const cancelled = error.kind === "cancelled";
  let primary: ReactNode = null;
  switch (recovery) {
    case "rename":
      primary = (
        <button type="button" className="btn btn--primary" onClick={onRename}>
          Use a free name
        </button>
      );
      break;
    case "choose_folder":
      primary = (
        <button type="button" className="btn btn--primary" onClick={onChooseFolder}>
          <IconFolder />
          Choose another folder
        </button>
      );
      break;
    case "retry":
      primary = (
        <button type="button" className="btn btn--primary" onClick={onRetry}>
          <IconRetry />
          {cancelled ? "Convert again" : "Try again"}
        </button>
      );
      break;
    case "choose_file":
      primary = (
        <button type="button" className="btn btn--primary" onClick={onChooseFile}>
          <IconOpen />
          Choose another file
        </button>
      );
      break;
    default:
      break;
  }
  return (
    <section className={`status ${cancelled ? "" : "status--error"}`} aria-labelledby="error-title" role={cancelled ? undefined : "alert"}>
      <div className={`status__icon ${cancelled ? "" : "status__icon--error"}`} aria-hidden="true">
        <IconAlert />
      </div>
      <h2 id="error-title" className="status__title">
        {ERROR_TITLE[error.kind] ?? "Something went wrong"}
      </h2>
      <p className="status__message selectable">{cancelled ? "Nothing was written to disk." : error.message}</p>
      <div className="button-row">
        {primary}
        <button type="button" className="btn btn--ghost" onClick={onBack}>
          Back to options
        </button>
      </div>
    </section>
  );
}
