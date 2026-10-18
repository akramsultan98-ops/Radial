import type { Backend } from "../lib/backend";
import { FORMAT_LABEL, STAGES } from "../lib/format";
import type { Edits, FileInfo, Format, JobStage } from "../lib/types";
import { usePreview } from "../state/usePreview";
import { IconArrowRight } from "./Icons";

interface Props {
  backend: Backend | null;
  file: FileInfo;
  edits: Edits;
  target: Format | null;
  stage: JobStage | null;
}

/** The selected file, shown inside the radial ring. */
export function FileOrb({ backend, file, edits, target, stage }: Props) {
  const preview = usePreview(backend, file.path, edits, 320);
  const from = file.format ? FORMAT_LABEL[file.format] : "?";
  const done = stage ? STAGES.indexOf(stage) : 0;
  const circ = 2 * Math.PI * 48;

  return (
    <div className="orb">
      <div className={`orb__thumb${preview.loading && !preview.url ? " orb__thumb--loading" : ""}`}>
        {preview.url && <img src={preview.url} alt="" draggable={false} />}
      </div>
      <div className="orb__name truncate" title={file.name}>
        {file.name}
      </div>
      {target ? (
        <div className="orb__route">
          {from}
          <IconArrowRight />
          {FORMAT_LABEL[target]}
        </div>
      ) : (
        <div className="orb__meta">
          <span className="badge">{from}</span>
          {file.width && file.height ? `${file.width} × ${file.height}` : null}
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
