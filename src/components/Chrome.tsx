// App chrome: top bar, step indicator, status bar and the notice banner.

import type { Phase } from "../state/workflow";
import { IconClose, IconHistory, IconOpen, IconSettings, IconShield } from "./Icons";

const STEPS = [
  { key: "file", label: "File" },
  { key: "format", label: "Format" },
  { key: "options", label: "Options" },
  { key: "save", label: "Save" },
] as const;

function stepIndex(phase: Phase): number {
  switch (phase) {
    case "empty":
    case "rejected":
      return 0;
    case "choose":
      return 1;
    case "configure":
    case "failed":
      return 2;
    case "converting":
    case "done":
      return 3;
  }
}

interface TopBarProps {
  phase: Phase;
  busy: boolean;
  onOpen: () => void;
  onHistory: () => void;
  onSettings: () => void;
  historyCount: number;
}

export function TopBar({ phase, busy, onOpen, onHistory, onSettings, historyCount }: TopBarProps) {
  const current = stepIndex(phase);
  const finished = phase === "done";
  return (
    <header className="topbar">
      <div className="brand">
        <svg className="brand__mark" viewBox="0 0 32 32" aria-hidden="true">
          <g fill="none" strokeWidth="3.4" transform="translate(16 16)">
            <path d="M -5.3 -9.6 A 11 11 0 0 1 5.3 -9.6" stroke="var(--accent)" />
            {[72, 144, 216, 288].map((r) => (
              <path key={r} d="M -5.3 -9.6 A 11 11 0 0 1 5.3 -9.6" stroke="var(--text-3)" transform={`rotate(${r})`} />
            ))}
          </g>
          <circle cx="16" cy="16" r="3.2" fill="var(--text-1)" />
        </svg>
        <span className="brand__name">RADIAL</span>
      </div>

      <ol className="steps" aria-label="Progress">
        {STEPS.map((s, i) => (
          <li
            key={s.key}
            className={i < current || finished ? "steps__item--done" : i === current ? "steps__item--current" : undefined}
            aria-current={i === current && !finished ? "step" : undefined}
          >
            <span className="steps__num" aria-hidden="true">
              {i + 1}
            </span>
            {s.label}
          </li>
        ))}
      </ol>

      <div className="topbar__actions">
        <button type="button" className="btn btn--ghost" onClick={onOpen} disabled={busy}>
          <IconOpen />
          Open
          <kbd>Ctrl O</kbd>
        </button>
        <button
          type="button"
          className="icon-btn"
          onClick={onHistory}
          aria-label={`Recent conversions (${historyCount})`}
          data-tip="Recent conversions"
          data-tip-pos="left"
        >
          <IconHistory />
        </button>
        <button type="button" className="icon-btn" onClick={onSettings} aria-label="Settings" data-tip="Settings" data-tip-pos="left">
          <IconSettings />
        </button>
      </div>
    </header>
  );
}

export function StatusBar({ message, engine }: { message: string; engine: string }) {
  return (
    <footer className="statusbar">
      <span className="statusbar__privacy">
        <IconShield />
        Local processing — files never leave this computer
      </span>
      <span className="statusbar__msg" role="status" aria-live="polite">
        {message}
      </span>
      <span className="statusbar__engine">{engine}</span>
    </footer>
  );
}

export function Notice({ text, onDismiss }: { text: string; onDismiss: () => void }) {
  return (
    <div className="notice" role="status">
      <span>{text}</span>
      <button type="button" className="icon-btn" onClick={onDismiss} aria-label="Dismiss">
        <IconClose />
      </button>
    </div>
  );
}
