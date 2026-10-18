import { useId, useState } from "react";
import { shortenPath } from "../lib/format";
import type { Settings } from "../lib/types";
import { IconFolder, IconShield } from "./Icons";
import { Overlay } from "./Overlay";

interface Props {
  settings: Settings;
  onClose: () => void;
  onSave: (s: Settings) => void;
  pickDirectory: () => Promise<string | null>;
}

const fill = (v: number) => ({ ["--fill" as string]: `${v}%` });

export function SettingsDialog({ settings, onClose, onSave, pickDirectory }: Props) {
  const id = useId();
  const [s, setS] = useState(settings);
  const set = (patch: Partial<Settings>) => setS((p) => ({ ...p, ...patch }));

  return (
    <Overlay
      title="Settings"
      variant="dialog"
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn btn--primary" onClick={() => onSave(s)}>
            Save
          </button>
        </>
      }
    >
      <div className="settings">
        <section>
          <h3 className="section-label">Saving</h3>
          <div className="row">
            <span className="row__label">
              Default destination
              <span className="field__hint selectable" title={s.defaultDestination ?? undefined}>
                {s.defaultDestination ? shortenPath(s.defaultDestination, 40) : "Same folder as the original"}
              </span>
            </span>
            <div className="button-row button-row--tight">
              {s.defaultDestination && (
                <button type="button" className="btn btn--ghost" onClick={() => set({ defaultDestination: null })}>
                  Reset
                </button>
              )}
              <button
                type="button"
                className="btn"
                onClick={async () => {
                  const d = await pickDirectory();
                  if (d) set({ defaultDestination: d });
                }}
              >
                <IconFolder />
                Choose…
              </button>
            </div>
          </div>
        </section>

        <section>
          <h3 className="section-label">Defaults</h3>
          <div className="field">
            <label className="field__label" htmlFor={`${id}-jq`}>
              JPEG quality <span className="field__value">{s.jpegQuality}</span>
            </label>
            <input
              id={`${id}-jq`}
              type="range"
              min={1}
              max={100}
              value={s.jpegQuality}
              style={fill(s.jpegQuality)}
              onChange={(e) => set({ jpegQuality: Number(e.target.value) })}
            />
          </div>
          <div className="field">
            <label className="field__label" htmlFor={`${id}-wq`}>
              WebP quality <span className="field__value">{s.webpQuality}</span>
            </label>
            <input
              id={`${id}-wq`}
              type="range"
              min={1}
              max={100}
              value={s.webpQuality}
              style={fill(s.webpQuality)}
              onChange={(e) => set({ webpQuality: Number(e.target.value) })}
            />
          </div>
          <div className="row">
            <label className="row__label" htmlFor={`${id}-bg`}>
              Background for transparency
              <span className="field__hint">Used for JPEG and BMP</span>
            </label>
            <span className="color">
              <input id={`${id}-bg`} type="color" value={s.background} onChange={(e) => set({ background: e.target.value })} />
              <span className="mono">{s.background.toUpperCase()}</span>
            </span>
          </div>
        </section>

        <section>
          <h3 className="section-label">Interface</h3>
          <div className="row">
            <span className="row__label" id={`${id}-motion`}>
              Motion
            </span>
            <div className="segmented" role="radiogroup" aria-labelledby={`${id}-motion`}>
              {(["system", "reduced", "full"] as const).map((m) => (
                <button key={m} type="button" role="radio" aria-checked={s.motion === m} onClick={() => set({ motion: m })}>
                  {m === "system" ? "System" : m === "reduced" ? "Reduced" : "Full"}
                </button>
              ))}
            </div>
          </div>
        </section>

        <section>
          <h3 className="section-label">Privacy</h3>
          <div className="row">
            <span className="row__label" id={`${id}-hist`}>
              Keep conversion history
              <span className="field__hint">File names and locations only, stored on this computer</span>
            </span>
            <button
              type="button"
              role="switch"
              className="switch"
              aria-checked={s.keepHistory}
              aria-labelledby={`${id}-hist`}
              onClick={() => set({ keepHistory: !s.keepHistory })}
            />
          </div>
          <p className="note">
            <IconShield />
            RADIAL works offline. It has no accounts, telemetry or analytics, and never uploads your files.
          </p>
        </section>
      </div>
    </Overlay>
  );
}
