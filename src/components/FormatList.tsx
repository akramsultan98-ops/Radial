// Conventional list of output formats — the accessible, non-radial way to
// choose. Built from the same capability data as the ring.

import { FORMAT_HINT } from "../lib/format";
import type { Capability, Format } from "../lib/types";
import { FormatIcon } from "./Icons";

interface Props {
  capabilities: Capability[] | null;
  selected: Format | null;
  disabled: boolean;
  /** Names only, three per row — used once a format has been chosen. */
  compact?: boolean;
  onSelect: (f: Format) => void;
}

export function FormatList({ capabilities, selected, disabled, compact = false, onSelect }: Props) {
  if (!capabilities) {
    return <p className="muted">Loading formats…</p>;
  }
  if (capabilities.length === 0) {
    return <p className="muted">No conversions are available for this file.</p>;
  }
  return (
    <div className={`format-list${compact ? " format-list--compact" : ""}`} role="radiogroup" aria-label="Convert to (list)">
      {capabilities.map((c) => (
        <button
          key={c.to}
          type="button"
          role="radio"
          aria-checked={selected === c.to}
          className="format-list__item"
          disabled={disabled || !c.available}
          onClick={() => onSelect(c.to)}
          title={c.available ? FORMAT_HINT[c.to] : c.engineStatus.state === "missing" ? c.engineStatus.hint : "Unavailable"}
        >
          <FormatIcon format={c.to} />
          <span className="format-list__name">{c.toLabel}</span>
          {!compact && <span className="format-list__hint">{c.available ? FORMAT_HINT[c.to] : "Engine not installed"}</span>}
        </button>
      ))}
    </div>
  );
}
