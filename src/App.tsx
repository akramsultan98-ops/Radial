import { useCallback, useEffect, useReducer, useRef, useState, type ReactNode } from "react";
import { Notice, StatusBar, TopBar } from "./components/Chrome";
import { AnnotationLayer, RedactLayer } from "./components/EditorLayers";
import { EmptyState } from "./components/EmptyState";
import { FileDetails } from "./components/FileDetails";
import { FileOrb } from "./components/FileOrb";
import { HistoryDrawer } from "./components/HistoryDrawer";
import {
  FormatIcon,
  IconAlert,
  IconBack,
  IconCheck,
  IconCompare,
  IconCrop,
  IconOpen,
  IconRotateLeft,
  IconRotateRight,
  OperationIcon,
} from "./components/Icons";
import { DEFAULT_TOOL, PageNav, type AnnotateTool } from "./components/OperationPanels";
import { OperationList } from "./components/OperationList";
import { blockedReason, OperationSettings, runLabel } from "./components/OperationSettings";
import { editedSize } from "./components/OptionsPanel";
import { OutputPanel, stemProblem } from "./components/OutputPanel";
import { PreviewView } from "./components/PreviewView";
import { SettingsDialog } from "./components/SettingsDialog";
import { ErrorPanel, ProgressPanel, ResultPanel } from "./components/StatusPanels";
import { getBackend, toRadialError, type Backend } from "./lib/backend";
import { FORMAT_LABEL, newJobId, PREVIEWABLE } from "./lib/format";
import type { Edits, Format, HistoryEntry, ImportResult, OperationId, Settings } from "./lib/types";
import { NO_EDITS } from "./lib/types";
import { RadialMenu, type RadialItem } from "./radial/RadialMenu";
import {
  activeCapability,
  activeFile,
  activeOperation,
  initialState,
  isUsable,
  outputFormat,
  readyToRun,
  reducer,
  type Output,
} from "./state/workflow";

const DEFAULT_SETTINGS: Settings = {
  defaultDestination: null,
  jpegQuality: 90,
  webpQuality: 85,
  background: "#ffffff",
  keepHistory: true,
  motion: "system",
};

const FULL_CROP = { x: 0, y: 0, width: 1, height: 1 };
const START_CROP = { x: 0.05, y: 0.05, width: 0.9, height: 0.9 };

export function App() {
  const [backend, setBackend] = useState<Backend | null>(null);
  const [state, dispatch] = useReducer(reducer, initialState);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [sources, setSources] = useState<Format[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [overlay, setOverlay] = useState<null | "history" | "settings">(null);
  const [dragOver, setDragOver] = useState(false);
  const [status, setStatus] = useState("");
  const [cancelling, setCancelling] = useState(false);
  const [cropping, setCropping] = useState(false);
  const [compare, setCompare] = useState<"output" | "source">("output");
  const [tool, setTool] = useState<AnnotateTool>(DEFAULT_TOOL);
  const [ringSize, setRingSize] = useState(520);
  const stageRef = useRef<HTMLDivElement>(null);

  const file = activeFile(state);
  const op = activeOperation(state);
  const cap = activeCapability(state);
  const item = state.tray[state.active];
  const busy = state.phase === "converting";
  const outcome = state.outcome;
  const outFormat = outputFormat(state);
  const isPdf = file?.format === "pdf";
  const pages = file?.pages ?? 1;
  const page = isPdf ? state.pdf.page : 0;

  // ---- Boot ----
  useEffect(() => {
    let alive = true;
    void getBackend().then(async (b) => {
      if (!alive) return;
      setBackend(b);
      const [s, src, h, launch] = await Promise.all([
        b.getSettings().catch(() => DEFAULT_SETTINGS),
        b.sourceFormats().catch(() => []),
        b.history().catch(() => []),
        b.takeLaunchFiles().catch(() => [] as ImportResult[]),
      ]);
      if (!alive) return;
      setSettings(s);
      setSources(src);
      setHistory(h);
      if (launch.length) dispatch({ type: "imported", items: launch });
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!backend) return;
    const offs = [
      backend.onJobStage((jobId, stage) => dispatch({ type: "job_stage", jobId, stage })),
      backend.onFilesDropped((items) => {
        setDragOver(false);
        dispatch({ type: "imported", items });
      }),
      backend.onDragState((s) => setDragOver(s === "over")),
    ];
    return () => offs.forEach((off) => off());
  }, [backend]);

  useEffect(() => {
    document.documentElement.dataset.motion = settings.motion;
  }, [settings.motion]);

  // Ring size follows the stage.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      if (!e) return;
      const { width, height } = e.contentRect;
      setRingSize(Math.round(Math.max(300, Math.min(620, width - 64, height - 110))));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Operations for the active file, from the registry.
  // Reload whenever the list is missing (also when the same file is re-imported).
  const fileKey = file && isUsable(item) && state.operations === null ? file.path : null;
  useEffect(() => {
    if (!backend || !fileKey) return;
    let alive = true;
    backend
      .operations(fileKey)
      .then((ops) => alive && dispatch({ type: "operations_loaded", operations: ops }))
      .catch((e: unknown) => alive && dispatch({ type: "operations_failed", error: toRadialError(e) }));
    return () => {
      alive = false;
    };
  }, [backend, fileKey]);

  // Suggest a destination once the operation and output format are known.
  useEffect(() => {
    if (!backend || !file || !state.operation || !outFormat || state.output) return;
    let alive = true;
    backend
      .suggestOutput(file.path, state.operation, outFormat)
      .then((s) => alive && dispatch({ type: "output_suggested", suggestion: s }))
      .catch((e: unknown) => alive && setStatus(toRadialError(e).message));
    return () => {
      alive = false;
    };
  }, [backend, file, state.operation, outFormat, state.output]);

  // Leave crop/compare modes when the file or operation changes.
  const modeKey = `${file?.path ?? ""}|${state.operation ?? ""}`;
  const [seenMode, setSeenMode] = useState(modeKey);
  if (modeKey !== seenMode) {
    setSeenMode(modeKey);
    setCropping(state.operation === "crop");
    setCompare("output");
  }

  const refreshHistory = useCallback(() => {
    backend?.history().then(setHistory).catch(() => {});
  }, [backend]);

  // ---- Actions ----
  const pickFiles = useCallback(async () => {
    if (!backend || busy) return;
    try {
      const items = await backend.pickFiles();
      if (items.length) dispatch({ type: "imported", items });
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  }, [backend, busy]);

  const chooseOperation = useCallback(
    (id: OperationId) => {
      dispatch({ type: "operation_chosen", operation: id, settings });
      if (id === "crop" && !state.options.edits.crop) {
        dispatch({ type: "edits_changed", patch: { crop: START_CROP } });
      }
      const o = state.operations?.find((x) => x.id === id);
      if (o) setStatus(o.available ? `${o.label}: ${o.description}` : (o.reason ?? ""));
    },
    [settings, state.options.edits.crop, state.operations],
  );

  const chooseTarget = useCallback(
    (target: Format) => {
      dispatch({ type: "target_chosen", target, settings });
      setStatus(`Output format: ${FORMAT_LABEL[target]}`);
    },
    [settings],
  );

  const run = useCallback(
    async (override?: Partial<Output>) => {
      if (!backend || !file || !state.operation || !state.output) return;
      const output = { ...state.output, ...override };
      const problem = stemProblem(output.stem);
      if (problem) {
        setStatus(problem);
        return;
      }
      if (override) dispatch({ type: "output_changed", patch: override });
      const jobId = newJobId();
      dispatch({ type: "convert_started", jobId });
      setCropping(false);
      setStatus("Working…");
      try {
        const result = await backend.runOperation(jobId, {
          source: file.path,
          operation: state.operation,
          target: state.operation === "convert" ? state.target : null,
          destDir: output.destDir,
          stem: output.stem.trim(),
          options: state.options,
          metadata: state.metadata,
          maxDimension: state.maxDimension,
          pdf: state.pdf,
        });
        dispatch({ type: "convert_succeeded", jobId, outcome: result });
        setCompare("output");
        setStatus(`Saved ${result.outputPath.split(/[\\/]/).pop()}`);
      } catch (e) {
        const err = toRadialError(e);
        dispatch({ type: "convert_failed", jobId, error: err });
        setStatus(err.kind === "cancelled" ? "Cancelled" : `Failed: ${err.message}`);
      } finally {
        setCancelling(false);
        refreshHistory();
      }
    },
    [
      backend,
      file,
      state.operation,
      state.output,
      state.target,
      state.options,
      state.metadata,
      state.maxDimension,
      state.pdf,
      refreshHistory,
    ],
  );

  const cancel = useCallback(() => {
    if (!backend || !state.job) return;
    setCancelling(true);
    void backend.cancel(state.job.id);
  }, [backend, state.job]);

  const chooseFolder = useCallback(async () => {
    if (!backend) return;
    try {
      const dir = await backend.pickDirectory();
      if (dir) {
        dispatch({ type: "output_changed", patch: { destDir: dir } });
        if (state.phase === "failed") dispatch({ type: "dismiss_error" });
      }
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  }, [backend, state.phase]);

  const useFreeName = useCallback(async () => {
    if (!backend || !file || !state.operation || !outFormat || !state.output) return;
    try {
      const s = await backend.suggestOutput(file.path, state.operation, outFormat, state.output.destDir, state.output.stem.trim());
      void run({ stem: s.stem });
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  }, [backend, file, state.operation, outFormat, state.output, run]);

  const report = (p: Promise<void> | undefined, ok: string) =>
    p?.then(() => setStatus(ok)).catch((e: unknown) => setStatus(toRadialError(e).message));

  const retryFromHistory = async (id: string) => {
    if (!backend || busy) return;
    try {
      const plan = await backend.retry(id);
      setOverlay(null);
      dispatch({
        type: "retry_loaded",
        item: plan.file,
        operation: plan.operation,
        target: plan.target,
        output: { destDir: plan.destDir, stem: plan.stem },
        options: plan.options,
      });
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  };

  const saveSettings = async (s: Settings) => {
    if (!backend) return;
    try {
      setSettings(await backend.setSettings(s));
      setOverlay(null);
      setStatus("Settings saved");
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  };

  const setEdits = (patch: Partial<Edits>) => dispatch({ type: "edits_changed", patch });

  const startCrop = () => {
    setCropping(true);
    dispatch({ type: "view_changed", view: "edit" });
    if (!state.options.edits.crop) setEdits({ crop: START_CROP });
  };

  const finishCrop = () => {
    const c = state.options.edits.crop;
    if (c && c.width > 0.995 && c.height > 0.995) setEdits({ crop: null });
    setCropping(false);
  };

  const back = () => {
    if (state.operation === "convert" && state.target) dispatch({ type: "target_cleared" });
    else dispatch({ type: "operation_cleared" });
  };

  // ---- Keyboard ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (overlay) return;
      const mod = e.ctrlKey || e.metaKey;
      const typing = (e.target as HTMLElement | null)?.closest("input, textarea");
      if (mod && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void pickFiles();
      } else if (mod && e.key === "Enter") {
        if (readyToRun(state) && op && file && !blockedReason(op, state, file)) {
          e.preventDefault();
          void run();
        }
      } else if (mod && e.key.toLowerCase() === "h") {
        e.preventDefault();
        setOverlay("history");
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setOverlay("settings");
      } else if (e.key === "Escape" && !typing) {
        if (busy) cancel();
        else if (cropping && state.operation !== "crop") finishCrop();
        else if (state.view === "edit" && state.operation === "convert") dispatch({ type: "view_changed", view: "radial" });
        else if (state.phase === "configure") back();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // ---- Stage ----
  const sourceLabel = file?.format ? FORMAT_LABEL[file.format] : "";
  const routeLabel =
    state.operation === "convert" ? (state.target ? FORMAT_LABEL[state.target] : null) : (op?.label ?? null);
  const formatRing = state.operation === "convert" && state.view !== "result";

  const backToOperations = (
    <button type="button" className="btn btn--ghost" onClick={() => dispatch({ type: "operation_cleared" })} disabled={busy}>
      <IconBack />
      Actions
    </button>
  );

  const rotateButtons = (e: Edits) => (
    <>
      <button
        type="button"
        className="icon-btn"
        aria-label="Rotate left"
        data-tip="Rotate left"
        onClick={() => setEdits({ rotate: ((e.rotate + 270) % 360) as Edits["rotate"], crop: null })}
      >
        <IconRotateLeft />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label="Rotate right"
        data-tip="Rotate right"
        onClick={() => setEdits({ rotate: ((e.rotate + 90) % 360) as Edits["rotate"], crop: null })}
      >
        <IconRotateRight />
      </button>
    </>
  );

  // PDF pages are shown at 2 px per point (144 DPI) so zooming stays sharp.
  const pageSize = (p: number) => {
    const s = file?.pageSizes[p];
    return s ? { width: Math.round(s[0] * 2), height: Math.round(s[1] * 2) } : null;
  };
  const pageCaption = (p: number, size = file?.pageSizes[p]) =>
    size
      ? `Page ${p + 1} of ${pages} · ${Math.round((size[0] / 72) * 25.4)} × ${Math.round((size[1] / 72) * 25.4)} mm`
      : undefined;
  const pageNav = isPdf ? (
    <PageNav page={page} pages={pages} onPage={(p) => dispatch({ type: "page_changed", page: p })} />
  ) : null;

  let stage: ReactNode;
  if (state.phase === "empty") {
    stage = <EmptyState sources={sources} dragOver={dragOver} onPick={pickFiles} />;
  } else if (state.phase === "rejected" || !file) {
    stage = (
      <div className="rejected">
        <div className="rejected__orb" aria-hidden="true">
          <IconAlert />
        </div>
        <h1 className="empty__title">This file can't be opened</h1>
        <p className="empty__sub">
          {item?.type === "error" ? item.error.message : (item?.problem?.message ?? "Unsupported file.")}
        </p>
        <button type="button" className="btn btn--primary btn--lg" onClick={pickFiles}>
          <IconOpen />
          Choose another file
        </button>
      </div>
    );
  } else if (state.view === "result" && outcome && !PREVIEWABLE(outcome.format)) {
    const text = outcome.format === "txt";
    stage = (
      <div className="rejected">
        <div className="rejected__orb rejected__orb--ok" aria-hidden="true">
          <FormatIcon format={outcome.format} />
        </div>
        <h1 className="empty__title">{text ? "Text extracted" : "Document saved"}</h1>
        <p className="empty__sub">
          {text
            ? "Plain text has no layout to preview. Open the file to read it, or show it in its folder."
            : "RADIAL doesn't preview Word documents. Open the copy to check it, or show it in its folder."}
        </p>
        <button
          type="button"
          className="btn btn--primary btn--lg"
          onClick={() => report(backend?.openPath(outcome.outputPath), "Opened in the default app")}
        >
          <IconOpen />
          {text ? "Open text file" : "Open document"}
        </button>
      </div>
    );
  } else if (state.view === "result" && outcome) {
    const showSource = compare === "source";
    // PDF results: page through them like the source. A crop changes the
    // visible size of the cropped pages.
    const pdfResult = outcome.format === "pdf";
    const crop = state.operation === "crop" ? state.options.edits.crop : null;
    const cropped = pdfResult && crop && (state.pdf.allPages || state.pdf.page === page);
    const base = pageSize(page);
    const resultNatural =
      pdfResult && base
        ? cropped && crop
          ? { width: Math.round(base.width * crop.width), height: Math.round(base.height * crop.height) }
          : base
        : { width: outcome.width ?? file.width ?? 1, height: outcome.height ?? file.height ?? 1 };
    const sourceNatural = (isPdf && pageSize(page)) || { width: file.width ?? 1, height: file.height ?? 1 };
    const pagedView = showSource ? isPdf : pdfResult;
    stage = (
      <PreviewView
        backend={backend}
        path={showSource ? file.path : outcome.outputPath}
        edits={NO_EDITS}
        page={pagedView ? page : 0}
        caption={pagedView ? `Page ${page + 1} of ${pages}` : undefined}
        natural={showSource ? sourceNatural : resultNatural}
        label={showSource ? `Original · ${sourceLabel}` : `Result · ${FORMAT_LABEL[outcome.format]}`}
        toolbarStart={
          <div className="segmented" role="radiogroup" aria-label="Compare">
            <button type="button" role="radio" aria-checked={!showSource} onClick={() => setCompare("output")}>
              Result
            </button>
            <button type="button" role="radio" aria-checked={showSource} onClick={() => setCompare("source")}>
              Original
            </button>
          </div>
        }
        toolbarEnd={
          <>
            {(showSource ? isPdf : pdfResult) && pageNav}
            <IconCompare className="preview__deco" />
          </>
        }
      />
    );
  } else if (state.view === "edit" && op) {
    const e = state.options.edits;
    const o = state.options;
    const natural = (isPdf && pageSize(page)) || { width: file.width ?? 1, height: file.height ?? 1 };
    const caption = isPdf ? pageCaption(page) : undefined;
    if (op.id === "annotate") {
      stage = (
        <PreviewView
          backend={backend}
          path={file.path}
          edits={NO_EDITS}
          page={page}
          caption={caption}
          natural={natural}
          label="Annotate"
          toolbarStart={backToOperations}
          toolbarEnd={pageNav}
          overlay={(box) => (
            <AnnotationLayer
              box={box}
              shapes={o.annotations.filter((s) => (s.page ?? 0) === page)}
              tool={tool}
              page={page}
              onAdd={(s) => dispatch({ type: "shapes_changed", shapes: [...o.annotations, s] })}
            />
          )}
        />
      );
    } else if (op.id === "redact") {
      stage = (
        <PreviewView
          backend={backend}
          path={file.path}
          edits={NO_EDITS}
          page={page}
          caption={caption}
          natural={natural}
          label="Redact"
          toolbarStart={backToOperations}
          toolbarEnd={pageNav}
          overlay={(box) =>
            isPdf ? (
              <RedactLayer
                box={box}
                regions={state.pdf.regions.filter((r) => r.page === page)}
                onAdd={(r) =>
                  dispatch({ type: "pdf_changed", patch: { regions: [...state.pdf.regions, { ...r, page }] } })
                }
              />
            ) : (
              <RedactLayer
                box={box}
                regions={o.redactions}
                onAdd={(r) => dispatch({ type: "regions_changed", regions: [...o.redactions, r] })}
              />
            )
          }
        />
      );
    } else if (op.id === "background") {
      stage = (
        <PreviewView
          backend={backend}
          path={file.path}
          edits={NO_EDITS}
          natural={natural}
          label="Background"
          toolbarStart={backToOperations}
          frame={{ color: o.background ?? "#ffffff", padding: o.padding }}
        />
      );
    } else {
      // Edit, Crop, and Convert's optional edit preview.
      const isCrop = op.id === "crop" || cropping;
      const renderEdits = isPdf ? NO_EDITS : isCrop || op.id === "edit" ? { ...e, crop: null } : e;
      stage = (
        <PreviewView
          backend={backend}
          path={file.path}
          edits={renderEdits}
          page={page}
          caption={caption}
          natural={isPdf ? natural : editedSize(file, renderEdits)}
          label={isCrop ? "Crop" : op.id === "edit" ? "Edit" : "Edited original"}
          crop={isCrop ? { value: e.crop ?? FULL_CROP, onChange: (c) => setEdits({ crop: c }) } : null}
          toolbarStart={
            op.id === "convert" ? (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => {
                  finishCrop();
                  dispatch({ type: "view_changed", view: "radial" });
                }}
              >
                <IconBack />
                Formats
              </button>
            ) : (
              backToOperations
            )
          }
          toolbarEnd={
            <>
              {isPdf ? pageNav : rotateButtons(e)}
              {op.id === "convert" &&
                (cropping ? (
                  <>
                    <button type="button" className="btn btn--ghost" onClick={() => setEdits({ crop: null })}>
                      Clear
                    </button>
                    <button type="button" className="btn btn--primary" onClick={finishCrop}>
                      <IconCheck />
                      Apply crop
                    </button>
                  </>
                ) : (
                  <button type="button" className="icon-btn" aria-label="Crop" data-tip="Crop" data-tip-pos="left" onClick={startCrop}>
                    <IconCrop />
                  </button>
                ))}
            </>
          }
        />
      );
    }
  } else {
    const convertOp = state.operations?.find((x) => x.id === "convert");
    const items: RadialItem[] | null = formatRing
      ? (convertOp?.targets ?? []).map((c) => ({
          id: c.to,
          label: c.toLabel,
          icon: <FormatIcon format={c.to} width={24} height={24} />,
          available: c.available,
          action: `Convert to ${c.toLabel}`,
          reason: c.engineStatus.state === "missing" ? c.engineStatus.hint : null,
        }))
      : state.operations
        ? state.operations.map((o) => ({
            id: o.id,
            label: o.label,
            icon: <OperationIcon op={o.id} width={24} height={24} />,
            available: o.available,
            action: `${o.label}: ${o.description}`,
            reason: o.reason,
          }))
        : null;
    stage = (
      <div className="workspace">
        {items ? (
          <RadialMenu
            key={`${file.path}|${formatRing ? "formats" : "ops"}`}
            items={items}
            label={formatRing ? "Output format" : "Actions"}
            selected={formatRing ? state.target : state.operation}
            onSelect={(id) => (formatRing ? chooseTarget(id as Format) : chooseOperation(id as OperationId))}
            size={ringSize}
            busy={busy}
            sourceLabel={sourceLabel}
          >
            <FileOrb backend={backend} file={file} edits={state.options.edits} route={routeLabel} stage={state.job?.stage ?? null} />
          </RadialMenu>
        ) : (
          <div className="workspace__loading" style={{ width: ringSize, height: ringSize }} aria-busy="true" />
        )}
        <div className="workspace__hint">
          {formatRing && !busy && (
            <button type="button" className="btn btn--ghost" onClick={() => dispatch({ type: "operation_cleared" })}>
              <IconBack />
              All actions
            </button>
          )}
          <p>
            {busy
              ? "Working — you can cancel with Esc"
              : formatRing
                ? state.target
                  ? "Pick another format any time · Ctrl+Enter converts"
                  : "Drag the file onto a format, or click one"
                : state.operation
                  ? "Pick another action any time · Ctrl+Enter runs it"
                  : "Drag the file onto an action, click one, or use the arrow keys and Enter"}
          </p>
        </div>
      </div>
    );
  }

  // ---- Side panel ----
  let panel: ReactNode = null;
  if (state.phase === "empty") {
    panel = (
      <>
        <section className="panel__section">
          <h2 className="section-label">How it works</h2>
          <ol className="howto">
            <li>
              <span>1</span>Drop a file or choose one
            </li>
            <li>
              <span>2</span>Drag it onto an action in the ring
            </li>
            <li>
              <span>3</span>Adjust settings and save a copy
            </li>
          </ol>
        </section>
        <section className="panel__section">
          <div className="section-head">
            <h2 className="section-label">Recent</h2>
            {history.length > 0 && (
              <button type="button" className="link" onClick={() => setOverlay("history")}>
                View all
              </button>
            )}
          </div>
          {history.length === 0 ? (
            <p className="muted">Your work will appear here.</p>
          ) : (
            <ul className="recent">
              {history.slice(0, 5).map((h) => (
                <li key={h.id}>
                  <span className={`dot dot--${h.status}`} aria-hidden="true" />
                  <span className="truncate" title={h.sourceName}>
                    {h.sourceName}
                  </span>
                  <span className="recent__fmt">
                    {h.operation === "convert" ? `→ ${FORMAT_LABEL[h.targetFormat]}` : h.operation}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </>
    );
  } else if (item) {
    panel = (
      <>
        {state.notice && (
          <section className="panel__section">
            <Notice text={state.notice} onDismiss={() => dispatch({ type: "dismiss_notice" })} />
          </section>
        )}
        {state.tray.length > 1 && (
          <section className="panel__section">
            <h2 className="section-label">Files ({state.tray.length})</h2>
            <ul className="tray" role="listbox" aria-label="Imported files">
              {state.tray.map((t, i) => {
                const ok = isUsable(t);
                return (
                  <li key={`${t.name}-${i}`} role="option" aria-selected={i === state.active}>
                    <button
                      type="button"
                      className="tray__item"
                      disabled={busy}
                      onClick={() => dispatch({ type: "select_item", index: i })}
                    >
                      <span className={`dot ${ok ? "dot--succeeded" : "dot--failed"}`} aria-hidden="true" />
                      <span className="truncate">{t.name}</span>
                      {!ok && <span className="sr-only">(cannot be opened)</span>}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        )}
        <section className="panel__section">
          <FileDetails item={item} />
        </section>

        {state.phase === "converting" && state.job && (
          <section className="panel__section">
            <ProgressPanel stage={state.job.stage} onCancel={cancel} cancelling={cancelling} />
          </section>
        )}

        {state.phase === "done" && outcome && (
          <section className="panel__section">
            <ResultPanel
              outcome={outcome}
              onOpen={() => report(backend?.openPath(outcome.outputPath), "Opened in the default app")}
              onReveal={() => report(backend?.revealPath(outcome.outputPath), "Shown in folder")}
              onAnother={() => dispatch({ type: "convert_another" })}
              onNewFile={pickFiles}
            />
          </section>
        )}

        {state.phase === "failed" && state.error && (
          <section className="panel__section">
            <ErrorPanel
              error={state.error}
              onRename={useFreeName}
              onChooseFolder={chooseFolder}
              onRetry={() => void run()}
              onChooseFile={pickFiles}
              onBack={() => dispatch({ type: "dismiss_error" })}
            />
          </section>
        )}

        {state.phase === "choose" && (
          <section className="panel__section">
            <h2 className="section-label">Actions</h2>
            <OperationList operations={state.operations} selected={null} disabled={busy} onSelect={chooseOperation} />
          </section>
        )}

        {state.phase === "configure" && op && file && (
          <>
            <section className="panel__section panel__section--tight">
              <div className="op-head">
                <span className="op-head__icon" aria-hidden="true">
                  <OperationIcon op={op.id} />
                </span>
                <div>
                  <h2 className="op-head__title">{op.label}</h2>
                  <p className="field__hint">{op.description}</p>
                </div>
                <button
                  type="button"
                  className="link"
                  aria-label="Change action"
                  onClick={() => dispatch({ type: "operation_cleared" })}
                >
                  Change
                </button>
              </div>
            </section>
            <OperationSettings
              backend={backend}
              state={state}
              dispatch={dispatch}
              file={file}
              op={op}
              busy={busy}
              tool={tool}
              onTool={setTool}
              onChooseTarget={chooseTarget}
              onStartCrop={startCrop}
              onToggleEditor={() => {
                setCropping(false);
                dispatch({ type: "view_changed", view: state.view === "edit" ? "radial" : "edit" });
              }}
            />
            {outFormat && (
              <section className="panel__section">
                <h2 className="section-label">Save as</h2>
                <OutputPanel
                  backend={backend}
                  source={file.path}
                  operation={op.id}
                  target={outFormat}
                  output={state.output}
                  disabled={busy}
                  onChange={(patch) => dispatch({ type: "output_changed", patch })}
                  onChooseFolder={chooseFolder}
                />
              </section>
            )}
            {(op.limitations.length > 0 || (cap?.limitations.length ?? 0) > 0) && (
              <section className="panel__section">
                <details className="limits">
                  <summary>Good to know</summary>
                  <ul>
                    {[...new Set([...op.limitations, ...(cap?.limitations ?? [])])].map((l) => (
                      <li key={l}>{l}</li>
                    ))}
                  </ul>
                </details>
              </section>
            )}
          </>
        )}
      </>
    );
  }

  const blocked = state.phase === "configure" && op && file ? blockedReason(op, state, file) : null;
  const showRun = state.phase === "configure" && op !== null;

  return (
    <div className={`app${dragOver ? " app--drag-over" : ""}`}>
      <TopBar
        phase={state.phase}
        busy={busy}
        onOpen={pickFiles}
        onHistory={() => setOverlay("history")}
        onSettings={() => setOverlay("settings")}
        historyCount={history.length}
      />
      <main className="main">
        <div ref={stageRef} className="stage" aria-label="Workspace">
          {stage}
          {dragOver && state.phase !== "empty" && (
            <div className="drop-veil" aria-hidden="true">
              <p>{busy ? "Busy — drop again when the current task finishes" : "Release to open"}</p>
            </div>
          )}
        </div>
        <aside className="panel" aria-label="Details">
          <div className="panel__scroll">{panel}</div>
          {showRun && op && (
            <div className="panel__footer">
              {blocked && <p className="panel__blocked">{blocked}</p>}
              <button
                type="button"
                className="btn btn--primary btn--lg btn--block"
                onClick={() => void run()}
                disabled={!readyToRun(state) || !!blocked || !state.output || !!stemProblem(state.output.stem)}
              >
                {runLabel(op, state, state.target ? FORMAT_LABEL[state.target] : null)}
                <kbd className="kbd--on-accent">Ctrl ↵</kbd>
              </button>
            </div>
          )}
        </aside>
      </main>
      <StatusBar
        message={status}
        engine={
          op?.engineLabel ??
          (isPdf
            ? "PDF engine (PDFium)"
            : file?.format === "docx"
              ? "Built-in Word reader"
              : file?.format === "txt"
                ? "Built-in text engine"
                : "Built-in image engine")
        }
      />

      {overlay === "history" && (
        <HistoryDrawer
          entries={history}
          keepHistory={settings.keepHistory}
          onClose={() => setOverlay(null)}
          onOpen={(p) => report(backend?.openPath(p), "Opened in the default app")}
          onReveal={(p) => report(backend?.revealPath(p), "Shown in folder")}
          onRetry={retryFromHistory}
          onClear={() =>
            backend
              ?.clearHistory()
              .then(() => {
                setHistory([]);
                setStatus("History cleared");
              })
              .catch((e: unknown) => setStatus(toRadialError(e).message))
          }
        />
      )}
      {overlay === "settings" && backend && (
        <SettingsDialog
          settings={settings}
          onClose={() => setOverlay(null)}
          onSave={saveSettings}
          pickDirectory={() => backend.pickDirectory()}
        />
      )}
    </div>
  );
}
