// Conventional list of operations — the accessible, non-radial way to pick
// what to do with a file. Built from the same registry data as the ring.

import type { Operation, OperationId } from "../lib/types";
import { OperationIcon } from "./Icons";

interface Props {
  operations: Operation[] | null;
  selected: OperationId | null;
  disabled: boolean;
  compact?: boolean;
  onSelect: (op: OperationId) => void;
}

export function OperationList({ operations, selected, disabled, compact = false, onSelect }: Props) {
  if (!operations) return <p className="muted">Loading actions…</p>;
  if (operations.length === 0) return <p className="muted">No actions are available for this file.</p>;
  return (
    <div className={`format-list${compact ? " format-list--compact" : ""}`} role="radiogroup" aria-label="Actions (list)">
      {operations.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={selected === o.id}
          className="format-list__item"
          disabled={disabled || !o.available}
          onClick={() => onSelect(o.id)}
          title={o.available ? o.description : (o.reason ?? "Unavailable")}
        >
          <OperationIcon op={o.id} />
          <span className="format-list__name">{o.label}</span>
          {!compact && <span className="format-list__hint">{o.available ? o.description : "Not available"}</span>}
        </button>
      ))}
    </div>
  );
}
