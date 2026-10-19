// The focused settings panel for the chosen operation.

import type { Dispatch } from "react";
import type { Backend } from "../lib/backend";
import type { Edits, FileInfo, Operation } from "../lib/types";
import type { Action, State } from "../state/workflow";
import { FormatList } from "./FormatList";
import {
  AnnotatePanel,
  BackgroundPanel,
  CompressPanel,
  CropPanel,
  MetadataPanel,
  PdfExportOptions,
  PdfRedactPanel,
  RedactPanel,
  type AnnotateTool,
} from "./OperationPanels";
import { OptionsPanel } from "./OptionsPanel";

interface Props {
  backend: Backend | null;
  state: State;
  dispatch: Dispatch<Action>;
  file: FileInfo;
  op: Operation;
  busy: boolean;
  tool: AnnotateTool;
  onTool: (t: AnnotateTool) => void;
  onChooseTarget: (t: NonNullable<State["target"]>) => void;
  onStartCrop: () => void;
  onToggleEditor: () => void;
}

export function OperationSettings({
  backend,
  state,
  dispatch,
  file,
  op,
  busy,
  tool,
  onTool,
  onChooseTarget,
  onStartCrop,
  onToggleEditor,
}: Props) {
  const setEdits = (patch: Partial<Edits>) => dispatch({ type: "edits_changed", patch });
  const o = state.options;
  const isPdf = file.format === "pdf";
  const pages = file.pages ?? 1;

  switch (op.id) {
    case "convert": {
      const cap = op.targets.find((c) => c.to === state.target) ?? null;
      return (
        <>
          <section className="panel__section">
            <div className="section-head">
              <h2 className="section-label">Convert to</h2>
              {state.target && (
                <button type="button" className="link" onClick={() => dispatch({ type: "target_cleared" })}>
                  Clear
                </button>
              )}
            </div>
            <FormatList
              capabilities={op.targets}
              selected={state.target}
              disabled={busy}
              compact={state.target !== null}
              onSelect={onChooseTarget}
            />
          </section>
          {cap && isPdf && cap.options.some((x) => x.kind === "page" || x.kind === "dpi") && (
            <section className="panel__section">
              <h2 className="section-label">Export</h2>
              <PdfExportOptions
                pages={pages}
                page={state.pdf.page}
                dpi={state.pdf.dpi}
                dpiSpec={(() => {
                  const d = cap.options.find((x) => x.kind === "dpi");
                  return d && d.kind === "dpi" ? d : null;
                })()}
                onPage={(page) => dispatch({ type: "page_changed", page })}
                onDpi={(dpi) => dispatch({ type: "pdf_changed", patch: { dpi } })}
              />
            </section>
          )}
          {cap && (!isPdf || cap.options.some((x) => x.kind === "quality")) && (
            <section className="panel__section">
              <h2 className="section-label">Options</h2>
              <OptionsPanel
                specs={cap.options}
                file={file}
                options={o}
                disabled={busy}
                editing={state.view === "edit"}
                onOptions={(patch) => dispatch({ type: "options_changed", patch })}
                onEdits={setEdits}
                onResetEdits={() => dispatch({ type: "edits_reset" })}
                onToggleEditor={onToggleEditor}
                onCrop={onStartCrop}
              />
            </section>
          )}
        </>
      );
    }
    case "compress":
      return (
        <section className="panel__section">
          <h2 className="section-label">Compression</h2>
          <CompressPanel
            op={op}
            file={file}
            quality={o.quality}
            maxDimension={state.maxDimension}
            onQuality={(q) => dispatch({ type: "options_changed", patch: { quality: q } })}
            onMaxDimension={(value) => dispatch({ type: "max_dimension_changed", value })}
          />
        </section>
      );
    case "edit":
      return (
        <section className="panel__section">
          <h2 className="section-label">Adjustments</h2>
          <OptionsPanel
            specs={op.options}
            file={file}
            options={{ ...o, edits: { ...o.edits, crop: null } }}
            disabled={busy}
            editing
            hideEditorToggle
            onOptions={(patch) => dispatch({ type: "options_changed", patch })}
            onEdits={setEdits}
            onResetEdits={() => dispatch({ type: "edits_reset" })}
            onToggleEditor={onToggleEditor}
            onCrop={onStartCrop}
          />
        </section>
      );
    case "crop":
      return (
        <section className="panel__section">
          <h2 className="section-label">Area to keep</h2>
          <CropPanel
            crop={o.edits.crop}
            rotate={o.edits.rotate}
            onRotate={(rotate) => setEdits({ rotate, crop: null })}
            onStartCrop={onStartCrop}
            onClear={() => setEdits({ crop: null })}
            pdf={
              isPdf
                ? {
                    pages,
                    page: state.pdf.page,
                    allPages: state.pdf.allPages,
                    onAllPages: (allPages) => dispatch({ type: "pdf_changed", patch: { allPages } }),
                  }
                : null
            }
          />
        </section>
      );
    case "annotate":
      return (
        <section className="panel__section">
          <h2 className="section-label">Tools</h2>
          <AnnotatePanel
            tool={tool}
            shapes={o.annotations}
            onTool={onTool}
            onShapes={(shapes) => dispatch({ type: "shapes_changed", shapes })}
          />
        </section>
      );
    case "redact":
      return (
        <section className="panel__section">
          <h2 className="section-label">Areas</h2>
          {isPdf ? (
            <PdfRedactPanel
              regions={state.pdf.regions}
              terms={state.pdf.terms}
              pages={pages}
              onRegions={(regions) => dispatch({ type: "pdf_changed", patch: { regions } })}
              onTerms={(terms) => dispatch({ type: "pdf_changed", patch: { terms } })}
              onGoToPage={(page) => dispatch({ type: "page_changed", page })}
            />
          ) : (
            <RedactPanel regions={o.redactions} onRegions={(regions) => dispatch({ type: "regions_changed", regions })} />
          )}
        </section>
      );
    case "background":
      return (
        <section className="panel__section">
          <h2 className="section-label">Fill</h2>
          <BackgroundPanel
            file={file}
            color={o.background ?? "#ffffff"}
            padding={o.padding}
            onColor={(background) => dispatch({ type: "options_changed", patch: { background } })}
            onPadding={(padding) => dispatch({ type: "options_changed", patch: { padding } })}
          />
        </section>
      );
    case "metadata":
      return (
        <section className="panel__section">
          <h2 className="section-label">Fields</h2>
          <MetadataPanel
            backend={backend}
            path={file.path}
            format={file.format}
            edit={state.metadata}
            onEdit={(patch) => dispatch({ type: "metadata_changed", patch })}
          />
        </section>
      );
  }
}

/** Label of the primary action button. */
export function runLabel(op: Operation, state: State, targetLabel: string | null): string {
  switch (op.id) {
    case "convert":
      return targetLabel ? `Convert to ${targetLabel}` : "Choose a format";
    case "compress":
      return "Compress";
    case "edit":
      return "Save edited copy";
    case "crop":
      return "Save cropped copy";
    case "annotate":
      return "Save annotated copy";
    case "redact":
      return "Redact & save copy";
    case "background":
      return "Apply background";
    case "metadata":
      return state.metadata.stripAll ? "Save without metadata" : "Save metadata";
  }
}

/** Why the run button is disabled, if it is. */
export function blockedReason(op: Operation, state: State, file: FileInfo): string | null {
  const o = state.options;
  switch (op.id) {
    case "convert":
      return state.target ? null : "Choose a format first.";
    case "edit":
      return o.edits.rotate === 0 && o.edits.brightness === 0 && o.edits.contrast === 0 && !o.resize
        ? "Rotate, resize or adjust the image first."
        : null;
    case "crop":
      return o.edits.crop ? null : "Choose the area to keep.";
    case "annotate":
      return o.annotations.length ? null : "Draw at least one annotation.";
    case "redact":
      if (file.format === "pdf") {
        return state.pdf.regions.length || state.pdf.terms.length ? null : "Mark an area or enter text to remove.";
      }
      return o.redactions.length ? null : "Mark at least one area.";
    case "background":
      return !file.hasAlpha && o.padding === 0 ? "Add a border (this image has no transparency)." : null;
    case "metadata":
      if (!op.options.some((x) => x.kind === "metadata_fields")) return "Metadata can't be edited for this format.";
      return Object.keys(state.metadata.fields).length || state.metadata.stripAll ? null : "Change a field first.";
    default:
      return null;
  }
}
