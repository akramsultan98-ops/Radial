import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { Notice, StatusBar, TopBar } from "./components/Chrome";
import { EmptyState } from "./components/EmptyState";
import { FileDetails } from "./components/FileDetails";
import { FileOrb } from "./components/FileOrb";
import { FormatList } from "./components/FormatList";
import { HistoryDrawer } from "./components/HistoryDrawer";
import { IconAlert, IconBack, IconCheck, IconCompare, IconCrop, IconOpen, IconRotateLeft, IconRotateRight } from "./components/Icons";
import { editedSize, OptionsPanel } from "./components/OptionsPanel";
import { OutputPanel, stemProblem } from "./components/OutputPanel";
import { PreviewView } from "./components/PreviewView";
import { SettingsDialog } from "./components/SettingsDialog";
import { ErrorPanel, ProgressPanel, ResultPanel } from "./components/StatusPanels";
import { getBackend, toRadialError, type Backend } from "./lib/backend";
import { FORMAT_LABEL, newJobId } from "./lib/format";
import type { Edits, Format, HistoryEntry, ImportResult, Settings } from "./lib/types";
import { NO_EDITS } from "./lib/types";
import { RadialMenu } from "./radial/RadialMenu";
import { activeCapability, activeFile, initialState, isUsable, reducer, type Output } from "./state/workflow";

const DEFAULT_SETTINGS: Settings = {
  defaultDestination: null,
  jpegQuality: 90,
  webpQuality: 85,
  background: "#ffffff",
  keepHistory: true,
  motion: "system",
};

const FULL_CROP = { x: 0, y: 0, width: 1, height: 1 };

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
  const [ringSize, setRingSize] = useState(520);
  const stageRef = useRef<HTMLDivElement>(null);

  const file = activeFile(state);
  const cap = activeCapability(state);
  const item = state.tray[state.active];
  const busy = state.phase === "converting";
  const outcome = state.outcome;

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

  // Capabilities for the active file, from the registry.
  const fileKey = file && isUsable(item) ? `${file.path}|${file.format}` : null;
  useEffect(() => {
    if (!backend || !fileKey || !file?.format) return;
    let alive = true;
    backend
      .capabilities(file.format)
      .then((c) => alive && dispatch({ type: "capabilities_loaded", capabilities: c }))
      .catch((e: unknown) => alive && dispatch({ type: "capabilities_failed", error: toRadialError(e) }));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed on fileKey
  }, [backend, fileKey]);

  // Suggest a destination once a target is chosen.
  useEffect(() => {
    if (!backend || !file || !state.target || state.output) return;
    let alive = true;
    backend
      .suggestOutput(file.path, state.target)
      .then((s) => alive && dispatch({ type: "output_suggested", suggestion: s }))
      .catch((e: unknown) => alive && setStatus(toRadialError(e).message));
    return () => {
      alive = false;
    };
  }, [backend, file, state.target, state.output]);

  // Leave crop/compare modes when the active file changes.
  const [seenPath, setSeenPath] = useState<string | null>(null);
  if ((file?.path ?? null) !== seenPath) {
    setSeenPath(file?.path ?? null);
    setCropping(false);
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

  const chooseTarget = useCallback(
    (target: Format) => {
      dispatch({ type: "target_chosen", target, settings });
      setStatus(`Output format: ${FORMAT_LABEL[target]}`);
    },
    [settings],
  );

  const convert = useCallback(
    async (override?: Partial<Output>) => {
      if (!backend || !file || !state.target || !state.output) return;
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
      setStatus("Converting…");
      try {
        const outcome = await backend.convert(jobId, {
          source: file.path,
          target: state.target,
          destDir: output.destDir,
          stem: output.stem.trim(),
          options: state.options,
        });
        dispatch({ type: "convert_succeeded", jobId, outcome });
        setCompare("output");
        setStatus(`Saved ${outcome.outputPath.split(/[\\/]/).pop()}`);
      } catch (e) {
        const err = toRadialError(e);
        dispatch({ type: "convert_failed", jobId, error: err });
        setStatus(err.kind === "cancelled" ? "Conversion cancelled" : `Conversion failed: ${err.message}`);
      } finally {
        setCancelling(false);
        refreshHistory();
      }
    },
    [backend, file, state.target, state.output, state.options, refreshHistory],
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
    if (!backend || !file || !state.target || !state.output) return;
    try {
      const s = await backend.suggestOutput(file.path, state.target, state.output.destDir, state.output.stem.trim());
      void convert({ stem: s.stem });
    } catch (e) {
      setStatus(toRadialError(e).message);
    }
  }, [backend, file, state.target, state.output, convert]);

  const run = (p: Promise<void> | undefined, ok: string) =>
    p?.then(() => setStatus(ok)).catch((e: unknown) => setStatus(toRadialError(e).message));

  const retryFromHistory = async (id: string) => {
    if (!backend || busy) return;
    try {
      const plan = await backend.retry(id);
      setOverlay(null);
      dispatch({
        type: "retry_loaded",
        item: plan.file,
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
    if (!state.options.edits.crop) setEdits({ crop: { x: 0.05, y: 0.05, width: 0.9, height: 0.9 } });
  };

  const finishCrop = () => {
    const c = state.options.edits.crop;
    if (c && c.width > 0.995 && c.height > 0.995) setEdits({ crop: null });
    setCropping(false);
  };

  // ---- Keyboard ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (overlay) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void pickFiles();
      } else if (mod && e.key === "Enter") {
        if (state.phase === "configure" || state.phase === "failed") {
          e.preventDefault();
          void convert();
        }
      } else if (mod && e.key.toLowerCase() === "h") {
        e.preventDefault();
        setOverlay("history");
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setOverlay("settings");
      } else if (e.key === "Escape") {
        if (busy) cancel();
        else if (cropping) finishCrop();
        else if (state.view === "edit") dispatch({ type: "view_changed", view: "radial" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // ---- Stage ----
  const sourceLabel = file?.format ? FORMAT_LABEL[file.format] : "";
  let stage: React.ReactNode;
  if (state.phase === "empty") {
    stage = <EmptyState sources={sources} dragOver={dragOver} onPick={pickFiles} />;
  } else if (state.phase === "rejected" || !file) {
    stage = (
      <div className="rejected">
        <div className="rejected__orb" aria-hidden="true">
          <IconAlert />
        </div>
        <h1 className="empty__title">This file can't be converted</h1>
        <p className="empty__sub">
          {item?.type === "error" ? item.error.message : item?.problem?.message ?? "Unsupported file."}
        </p>
        <button type="button" className="btn btn--primary btn--lg" onClick={pickFiles}>
          <IconOpen />
          Choose another file
        </button>
      </div>
    );
  } else if (state.view === "result" && state.outcome) {
    const showSource = compare === "source";
    stage = (
      <PreviewView
        backend={backend}
        path={showSource ? file.path : state.outcome.outputPath}
        edits={NO_EDITS}
        natural={
          showSource
            ? { width: file.width ?? 1, height: file.height ?? 1 }
            : { width: state.outcome.width, height: state.outcome.height }
        }
        label={showSource ? `Original · ${sourceLabel}` : `Output · ${FORMAT_LABEL[state.outcome.format]}`}
        toolbarStart={
          <div className="segmented" role="radiogroup" aria-label="Compare">
            <button type="button" role="radio" aria-checked={!showSource} onClick={() => setCompare("output")}>
              Output
            </button>
            <button type="button" role="radio" aria-checked={showSource} onClick={() => setCompare("source")}>
              Original
            </button>
          </div>
        }
        toolbarEnd={<IconCompare className="preview__deco" />}
      />
    );
  } else if (state.view === "edit" && state.phase !== "converting") {
    const e = state.options.edits;
    const renderEdits = cropping ? { ...e, crop: null } : e;
    stage = (
      <PreviewView
        backend={backend}
        path={file.path}
        edits={renderEdits}
        natural={editedSize(file, renderEdits)}
        label={cropping ? "Crop" : "Edited original"}
        crop={cropping ? { value: e.crop ?? FULL_CROP, onChange: (c) => setEdits({ crop: c }) } : null}
        toolbarStart={
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
        }
        toolbarEnd={
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
            {cropping ? (
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
            )}
          </>
        }
      />
    );
  } else {
    stage = (
      <div className="workspace">
        {state.capabilities ? (
          <RadialMenu
            key={file.path}
            capabilities={state.capabilities}
            selected={state.target}
            onSelect={chooseTarget}
            size={ringSize}
            busy={busy}
            sourceLabel={sourceLabel}
          >
            <FileOrb
              backend={backend}
              file={file}
              edits={state.options.edits}
              target={state.target}
              stage={state.job?.stage ?? null}
            />
          </RadialMenu>
        ) : (
          <div className="workspace__loading" style={{ width: ringSize, height: ringSize }} aria-busy="true" />
        )}
        <p className="workspace__hint">
          {busy
            ? "Converting — you can cancel with Esc"
            : state.target
              ? "Pick another format any time · Ctrl+Enter converts"
              : "Drag the file onto a format, click one, or use the arrow keys and Enter"}
        </p>
      </div>
    );
  }

  // ---- Side panel ----
  let panel: React.ReactNode = null;
  if (state.phase === "empty") {
    panel = (
      <>
        <section className="panel__section">
          <h2 className="section-label">How it works</h2>
          <ol className="howto">
            <li>
              <span>1</span>Drop an image or choose one
            </li>
            <li>
              <span>2</span>Drag it onto a format in the ring
            </li>
            <li>
              <span>3</span>Adjust options and convert
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
            <p className="muted">Your conversions will appear here.</p>
          ) : (
            <ul className="recent">
              {history.slice(0, 5).map((h) => (
                <li key={h.id}>
                  <span className={`dot dot--${h.status}`} aria-hidden="true" />
                  <span className="truncate" title={h.sourceName}>
                    {h.sourceName}
                  </span>
                  <span className="recent__fmt">→ {FORMAT_LABEL[h.targetFormat]}</span>
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
                const name = t.type === "file" ? t.name : t.name;
                const ok = isUsable(t);
                return (
                  <li key={`${name}-${i}`} role="option" aria-selected={i === state.active}>
                    <button type="button" className="tray__item" disabled={busy} onClick={() => dispatch({ type: "select_item", index: i })}>
                      <span className={`dot ${ok ? "dot--succeeded" : "dot--failed"}`} aria-hidden="true" />
                      <span className="truncate">{name}</span>
                      {!ok && <span className="sr-only">(cannot be converted)</span>}
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

        {state.phase === "done" && outcome && file && (
          <section className="panel__section">
            <ResultPanel
              file={file}
              outcome={outcome}
              onOpen={() => run(backend?.openPath(outcome.outputPath), "Opened in the default app")}
              onReveal={() => run(backend?.revealPath(outcome.outputPath), "Shown in folder")}
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
              onRetry={() => void convert()}
              onChooseFile={pickFiles}
              onBack={() => dispatch({ type: "dismiss_error" })}
            />
          </section>
        )}

        {(state.phase === "choose" || state.phase === "configure") && (
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
              capabilities={state.capabilities}
              selected={state.target}
              disabled={busy}
              compact={state.target !== null}
              onSelect={chooseTarget}
            />
          </section>
        )}

        {state.phase === "configure" && cap && file && state.target && (
          <>
            <section className="panel__section">
              <h2 className="section-label">Options</h2>
              <OptionsPanel
                cap={cap}
                file={file}
                options={state.options}
                disabled={busy}
                editing={state.view === "edit"}
                onOptions={(patch) => dispatch({ type: "options_changed", patch })}
                onEdits={setEdits}
                onResetEdits={() => {
                  setCropping(false);
                  dispatch({ type: "edits_reset" });
                }}
                onToggleEditor={() => {
                  setCropping(false);
                  dispatch({ type: "view_changed", view: state.view === "edit" ? "radial" : "edit" });
                }}
                onCrop={startCrop}
              />
            </section>
            <section className="panel__section">
              <h2 className="section-label">Save as</h2>
              <OutputPanel
                backend={backend}
                source={file.path}
                target={state.target}
                output={state.output}
                disabled={busy}
                onChange={(patch) => dispatch({ type: "output_changed", patch })}
                onChooseFolder={chooseFolder}
              />
            </section>
            {cap.limitations.length > 0 && (
              <section className="panel__section">
                <details className="limits">
                  <summary>Good to know</summary>
                  <ul>
                    {cap.limitations.map((l) => (
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

  const convertTarget = state.phase === "configure" ? state.target : null;

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
              <p>{busy ? "Busy — drop again when the conversion finishes" : "Release to open"}</p>
            </div>
          )}
        </div>
        <aside className="panel" aria-label="Details">
          <div className="panel__scroll">{panel}</div>
          {convertTarget && (
            <div className="panel__footer">
              <button
                type="button"
                className="btn btn--primary btn--lg btn--block"
                onClick={() => void convert()}
                disabled={!state.output || !!stemProblem(state.output.stem)}
              >
                Convert to {FORMAT_LABEL[convertTarget]}
                <kbd className="kbd--on-accent">Ctrl ↵</kbd>
              </button>
            </div>
          )}
        </aside>
      </main>
      <StatusBar message={status} engine={cap?.engineLabel ?? "Built-in image engine"} />

      {overlay === "history" && (
        <HistoryDrawer
          entries={history}
          keepHistory={settings.keepHistory}
          onClose={() => setOverlay(null)}
          onOpen={(p) => run(backend?.openPath(p), "Opened in the default app")}
          onReveal={(p) => run(backend?.revealPath(p), "Shown in folder")}
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
