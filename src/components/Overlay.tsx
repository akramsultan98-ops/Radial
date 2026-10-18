// Accessible modal surface used by the history drawer and settings dialog:
// focus moves in on open, Tab is trapped, Escape closes, focus is restored.

import { useEffect, useRef, type ReactNode } from "react";
import { IconClose } from "./Icons";

interface Props {
  title: string;
  variant: "drawer" | "dialog";
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])';

export function Overlay({ title, variant, onClose, children, footer }: Props) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    panel.current?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    return () => previous?.focus?.();
  }, []);

  // Escape closes even when focus was lost (e.g. the focused button was removed).
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close.current();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab" || !panel.current) return;
    const items = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) return;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <div className={`overlay overlay--${variant}`} onKeyDown={onKeyDown}>
      <div className="overlay__scrim" onClick={onClose} aria-hidden="true" />
      <div ref={panel} className="overlay__panel" role="dialog" aria-modal="true" aria-labelledby="overlay-title">
        <header className="overlay__head">
          <h2 id="overlay-title">{title}</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Close">
            <IconClose />
          </button>
        </header>
        <div className="overlay__body">{children}</div>
        {footer && <footer className="overlay__foot">{footer}</footer>}
      </div>
    </div>
  );
}
