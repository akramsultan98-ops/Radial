// Destination folder and file name, with early collision feedback.

import { useEffect, useId, useState } from "react";
import type { Backend } from "../lib/backend";
import { FORMAT_EXT, shortenPath } from "../lib/format";
import type { Format, OperationId } from "../lib/types";
import type { Output } from "../state/workflow";
import { IconFolder } from "./Icons";

const INVALID_CHARS = '/\\:*?"<>|';
const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i;

/** Mirrors `naming::validate_stem` in the core; the core re-validates. */
export function stemProblem(stem: string): string | null {
  const s = stem.trim();
  if (!s) return "Enter a file name.";
  if (s.length > 200) return "That name is too long.";
  if (s === "." || s === "..") return "That name isn't allowed.";
  const bad = [...s].find((c) => INVALID_CHARS.includes(c) || c.charCodeAt(0) < 32);
  if (bad) return bad.charCodeAt(0) < 32 ? "Remove control characters." : `Names can't contain “${bad}”.`;
  if (s.endsWith(".")) return "Names can't end with a dot.";
  if (RESERVED.test(s)) return "That name is reserved by Windows.";
  return null;
}

interface Props {
  backend: Backend | null;
  source: string;
  operation: OperationId;
  target: Format;
  output: Output | null;
  disabled: boolean;
  onChange: (patch: Partial<Output>) => void;
  onChooseFolder: () => void;
}

export function OutputPanel({ backend, source, operation, target, output, disabled, onChange, onChooseFolder }: Props) {
  const id = useId();
  const [check, setCheck] = useState<{ key: string; alternative: string | null } | null>(null);
  const problem = output ? stemProblem(output.stem) : null;
  const key = output ? `${source}|${operation}|${target}|${output.destDir}|${output.stem.trim()}` : "";
  const taken = check?.key === key ? check.alternative : null;

  // Ask the backend whether the name is free (debounced).
  useEffect(() => {
    if (!backend || !output || problem) return;
    let alive = true;
    const stem = output.stem.trim();
    const t = setTimeout(() => {
      backend
        .suggestOutput(source, operation, target, output.destDir, stem)
        .then((s) => alive && setCheck({ key, alternative: s.stem !== stem ? s.stem : null }))
        .catch(() => alive && setCheck({ key, alternative: null }));
    }, 250);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [backend, source, operation, target, output, problem, key]);

  if (!output) return <p className="muted">Preparing destination…</p>;

  return (
    <div className="output">
      <div className="output__folder">
        <IconFolder />
        <span className="truncate mono selectable" title={output.destDir}>
          {shortenPath(output.destDir, 34)}
        </span>
        <button type="button" className="btn btn--ghost output__change" onClick={onChooseFolder} disabled={disabled}>
          Change…
        </button>
      </div>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-name`}>
          File name
        </label>
        <div className="output__name">
          <input
            id={`${id}-name`}
            className="input"
            value={output.stem}
            spellCheck={false}
            disabled={disabled}
            aria-invalid={problem || taken ? true : undefined}
            aria-describedby={`${id}-msg`}
            onChange={(e) => onChange({ stem: e.target.value })}
          />
          <span className="output__ext mono">.{FORMAT_EXT[target]}</span>
        </div>
        <span id={`${id}-msg`} className={`field__hint${problem || taken ? " field__hint--warn" : ""}`} aria-live="polite">
          {problem ??
            (taken ? (
              <>
                A file with this name exists.{" "}
                <button type="button" className="link" onClick={() => onChange({ stem: taken })}>
                  Use “{taken}”
                </button>
              </>
            ) : (
              "RADIAL never overwrites existing files or the original."
            ))}
        </span>
      </div>
    </div>
  );
}
