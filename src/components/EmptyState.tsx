import type { Format } from "../lib/types";
import { FORMAT_LABEL } from "../lib/format";
import { layoutSegments, sectorPath } from "../radial/geometry";
import { FormatIcon, IconOpen } from "./Icons";

interface Props {
  sources: Format[];
  dragOver: boolean;
  onPick: () => void;
}

export function EmptyState({ sources, dragOver, onPick }: Props) {
  return (
    <div className={`empty${dragOver ? " empty--over" : ""}`}>
      <div className="empty__target" aria-hidden="true">
        <svg viewBox="0 0 200 200" className="empty__ring">
          {layoutSegments(6, 14).map((s) => (
            <path key={s.index} d={sectorPath(100, 100, 78, 96, s.start, s.end)} style={{ ["--i" as string]: s.index }} />
          ))}
        </svg>
        <div className="empty__glyph">
          <IconOpen />
        </div>
      </div>
      <h1 className="empty__title">{dragOver ? "Release to open" : "Drop an image to begin"}</h1>
      <p className="empty__sub">
        Convert between formats in one gesture. Everything happens on this computer — nothing is uploaded.
      </p>
      <div className="empty__actions">
        <button type="button" className="btn btn--primary btn--lg" onClick={onPick}>
          <IconOpen />
          Choose file
        </button>
        <span className="empty__shortcut">
          or press <kbd>Ctrl</kbd> <kbd>O</kbd>
        </span>
      </div>
      {sources.length > 0 && (
        <ul className="empty__formats" aria-label="Formats RADIAL can open">
          {sources.map((f) => (
            <li key={f}>
              <FormatIcon format={f} />
              {FORMAT_LABEL[f]}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
