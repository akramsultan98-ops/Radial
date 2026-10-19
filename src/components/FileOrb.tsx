import type { Backend } from "../lib/backend";
import { FORMAT_LABEL, PREVIEWABLE, STAGES } from "../lib/format";
import type { Edits, FileInfo, JobStage } from "../lib/types";
import { usePreview } from "../state/usePreview";
import { FormatIcon, IconArrowRight } from "./Icons";

interface Props {
  backend: Backend | null;
  file: FileInfo;
  edits: Edits;
  /** Where the file is going, e.g. "JPEG" or "Compress". */
  route: string | null;
  stage: JobStage | null;
}

/** The selected file, shown inside the radial ring. */
export function FileOrb({ backend, file, edits, route, stage }: Props) {
  const canPreview = PREVIEWABLE(file.format);
  const preview = usePreview(canPreview ? backend : null, file.path, edits, 320);
  const from = file.format ? FORMAT_LABEL[file.format] : "?";
  const done = stage ? STAGES.indexOf(stage) : 0;
  const circ = 2 * Math.PI * 48;

  return (
    <div className="orb">
      <div
        className={`orb__thumb${file.hasAlpha ? "" : " orb__thumb--opaque"}${preview.loading && !preview.url ? " orb__thumb--loading" : ""}`}
      >
        {preview.url && <img src={preview.url} alt="" draggable={false} />}
        {!canPreview && file.format && <FormatIcon format={file.format} className="orb__doc-icon" />}
      </div>
      <div className="orb__name truncate" title={file.name}>
        {file.name}
      </div>
      {route ? (
        <div className="orb__route">
          {from}
          <IconArrowRight />
          {route}
        </div>
      ) : (
        <div className="orb__meta">
          <span className="badge">{from}</span>
          {file.format === "txt" || file.format === "docx"
            ? null
            : file.format === "pdf"
            ? `${file.pages ?? 1} page${file.pages === 1 ? "" : "s"}`
            : file.width && file.height
              ? `${file.width} × ${file.height}`
              : null}
        </div>
      )}
      {stage && (
        <svg className="orbit" viewBox="0 0 100 100" aria-hidden="true">
          <circle className="orbit__track" cx="50" cy="50" r="48" />
          <circle
            className="orbit__done"
            cx="50"
            cy="50"
            r="48"
            strokeDasharray={circ}
            strokeDashoffset={circ * (1 - done / STAGES.length)}
          />
          <circle className="orbit__arc" cx="50" cy="50" r="48" strokeDasharray={`${circ * 0.12} ${circ}`} />
        </svg>
      )}
    </div>
  );
}
