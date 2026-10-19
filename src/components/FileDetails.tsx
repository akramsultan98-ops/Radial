import { FORMAT_LABEL, formatBytes, formatDims, shortenPath } from "../lib/format";
import type { ImportResult } from "../lib/types";
import { IconAlert } from "./Icons";

function dirName(p: string) {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i > 0 ? p.slice(0, i) : p;
}

export function FileDetails({ item }: { item: ImportResult }) {
  if (item.type === "error") {
    return (
      <section className="card card--danger" aria-labelledby="file-title">
        <h2 id="file-title" className="card__title truncate" title={item.name}>
          {item.name || "Unknown file"}
        </h2>
        <p className="problem">
          <IconAlert />
          {item.error.message}
        </p>
      </section>
    );
  }
  const recognized = item.recognized?.kind === "known" ? item.recognized.format.label : null;
  const format = item.format ? FORMAT_LABEL[item.format] : recognized ?? "Unknown";
  return (
    <section className={`card${item.problem ? " card--danger" : ""}`} aria-labelledby="file-title">
      <div className="card__head">
        <h2 id="file-title" className="card__title truncate selectable" title={item.name}>
          {item.name}
        </h2>
        <span className="badge">{format}</span>
      </div>
      <p className="card__path truncate mono selectable" title={dirName(item.path)}>
        {shortenPath(dirName(item.path), 46)}
      </p>
      {item.format === "pdf" ? (
        <dl className="facts">
          <div>
            <dt>Size</dt>
            <dd>{formatBytes(item.size)}</dd>
          </div>
          <div>
            <dt>Pages</dt>
            <dd>{item.pages ?? "—"}</dd>
          </div>
          <div>
            <dt>Page size</dt>
            <dd>
              {item.width && item.height
                ? `${Math.round((item.width / 72) * 25.4)} × ${Math.round((item.height / 72) * 25.4)} mm`
                : "—"}
            </dd>
          </div>
          <div>
            <dt>Protection</dt>
            <dd>{item.restricted ? "Restricted" : item.signed ? "Signed" : "None"}</dd>
          </div>
        </dl>
      ) : (
      <dl className="facts">
        <div>
          <dt>Size</dt>
          <dd>{formatBytes(item.size)}</dd>
        </div>
        <div>
          <dt>Dimensions</dt>
          <dd>{formatDims(item.width, item.height)}</dd>
        </div>
        <div>
          <dt>Transparency</dt>
          <dd>{item.format ? (item.hasAlpha ? "Yes" : "No") : "—"}</dd>
        </div>
        <div>
          <dt>Depth</dt>
          <dd>{item.bitDepth ? `${item.bitDepth}-bit` : "—"}</dd>
        </div>
      </dl>
      )}
      {item.restricted && !item.problem && (
        <p className="note note--warning">
          <IconAlert />
          This PDF has an owner password that restricts changes. RADIAL can show its properties but won't modify it.
        </p>
      )}
      {item.signed && !item.problem && (
        <p className="note note--warning">
          <IconAlert />
          This PDF is digitally signed. A changed copy won't carry a valid signature.
        </p>
      )}
      {item.extensionMismatch && !item.problem && (
        <p className="note note--warning">
          <IconAlert />
          The file name says otherwise, but the contents are {format}. RADIAL uses the contents.
        </p>
      )}
      {item.problem && (
        <p className="problem" role="alert">
          <IconAlert />
          {item.problem.message}
        </p>
      )}
    </section>
  );
}
