import { useState } from "react";
import { FORMAT_LABEL, OPERATION_LABEL, timeAgo } from "../lib/format";
import type { HistoryEntry } from "../lib/types";
import { IconAlert, IconCheck, IconClose, IconExternal, IconFolder, IconRetry, IconTrash } from "./Icons";
import { Overlay } from "./Overlay";

interface Props {
  entries: HistoryEntry[];
  keepHistory: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
  onReveal: (path: string) => void;
  onRetry: (id: string) => void;
  onClear: () => void;
}

export function HistoryDrawer({ entries, keepHistory, onClose, onOpen, onReveal, onRetry, onClear }: Props) {
  const [confirming, setConfirming] = useState(false);
  return (
    <Overlay
      title="Recent activity"
      variant="drawer"
      onClose={onClose}
      footer={
        entries.length > 0 &&
        (confirming ? (
          <div className="confirm">
            <span>Remove all {entries.length} entries? Files are not deleted.</span>
            <button type="button" className="btn btn--ghost" onClick={() => setConfirming(false)}>
              Keep
            </button>
            <button
              type="button"
              className="btn btn--danger"
              onClick={() => {
                onClear();
                setConfirming(false);
              }}
            >
              Clear history
            </button>
          </div>
        ) : (
          <button type="button" className="btn btn--ghost" onClick={() => setConfirming(true)}>
            <IconTrash />
            Clear history
          </button>
        ))
      }
    >
      {!keepHistory && <p className="note">History is turned off in Settings. New tasks aren't recorded.</p>}
      {entries.length === 0 ? (
        <div className="history-empty">
          <p>Nothing here yet.</p>
          <p className="muted">Finished, failed and cancelled tasks appear here. Only names and locations are stored — never annotation text, redaction areas or metadata values.</p>
        </div>
      ) : (
        <ul className="history">
          {entries.map((e) => {
            const out = e.outputPath;
            return (
            <li key={e.id} className={`history__item history__item--${e.status}`}>
              <span className="history__status" aria-hidden="true">
                {e.status === "succeeded" ? <IconCheck /> : e.status === "failed" ? <IconAlert /> : <IconClose />}
              </span>
              <div className="history__main">
                <p className="history__name truncate" title={e.sourceName}>
                  {e.sourceName}
                </p>
                <p className="history__meta">
                  {e.operation === "convert"
                    ? `${FORMAT_LABEL[e.sourceFormat]} → ${FORMAT_LABEL[e.targetFormat]}`
                    : `${OPERATION_LABEL[e.operation]} · ${FORMAT_LABEL[e.targetFormat]}`}{" "}
                  · {timeAgo(e.timestamp)}
                  <span className="sr-only">
                    {" "}
                    · {e.status === "succeeded" ? "Succeeded" : e.status === "failed" ? "Failed" : "Cancelled"}
                  </span>
                </p>
                {e.status === "failed" && e.errorMessage && <p className="history__error">{e.errorMessage}</p>}
              </div>
              <div className="history__actions">
                {out ? (
                  <>
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={() => onOpen(out)}
                      aria-label={`Open ${e.stem}`}
                      data-tip="Open"
                    >
                      <IconExternal />
                    </button>
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={() => onReveal(out)}
                      aria-label={`Show ${e.stem} in folder`}
                      data-tip="Show in folder"
                      data-tip-pos="left"
                    >
                      <IconFolder />
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    className="icon-btn"
                    onClick={() => onRetry(e.id)}
                    aria-label={`Retry ${e.sourceName}`}
                    data-tip="Retry"
                    data-tip-pos="left"
                  >
                    <IconRetry />
                  </button>
                )}
              </div>
            </li>
            );
          })}
        </ul>
      )}
    </Overlay>
  );
}
