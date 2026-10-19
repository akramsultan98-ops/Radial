//! IPC commands exposed to the webview. Each validates every path against the
//! session allowlist before touching the file system.

use crate::state::{lock, AppState, ImportResult, PreviewEntry};
use radial_core::engine::{self, Stage};
use radial_core::history::{Entry, Status};
use radial_core::metadata::{self, MetadataReport};
use radial_core::ops::{self, Operation, OperationId, OperationOutcome, OperationRequest};
use radial_core::options::{ConvertOptions, Edits};
use radial_core::settings::Settings;
use radial_core::{detect, naming, ErrorKind, Format, RadialError, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

fn internal(msg: impl std::fmt::Display) -> RadialError {
    RadialError::new(ErrorKind::Internal, msg.to_string())
}

/// Run blocking work off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| internal(format!("Worker failed: {e}")))?
}

#[tauri::command]
pub async fn take_launch_files(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ImportResult>> {
    let files = state.take_launch_files();
    let app2 = app.clone();
    blocking(move || Ok(app2.state::<AppState>().import(&files))).await
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Result<Vec<ImportResult>> {
    blocking(move || {
        let sources = app.state::<AppState>().registry.sources();
        let exts: Vec<&str> = sources
            .iter()
            .flat_map(|f| f.extensions().iter().copied())
            .collect();
        let picked = app
            .dialog()
            .file()
            .set_title("Choose a file")
            .add_filter("Images and PDFs", &exts)
            .add_filter("All files", &["*"])
            .blocking_pick_files();
        let paths: Vec<PathBuf> = picked
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .collect();
        Ok(app.state::<AppState>().import(&paths))
    })
    .await
}

#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Result<Option<PathBuf>> {
    blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Choose where to save")
            .blocking_pick_folder();
        let dir = picked.and_then(|p| p.into_path().ok());
        if let Some(d) = &dir {
            lock(&app.state::<AppState>().access).grant_dir(d);
        }
        Ok(dir)
    })
    .await
}

/// Source formats with at least one registered conversion.
#[tauri::command]
pub fn source_formats(state: State<'_, AppState>) -> Vec<Format> {
    state.registry.sources()
}

/// Operations available for a file the user opened, from the registry.
#[tauri::command]
pub async fn operations_for(app: AppHandle, path: PathBuf) -> Result<Vec<Operation>> {
    app.state::<AppState>().ensure_file(&path)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let info = detect::inspect(&path)?;
        Ok(ops::operations_for(&state.registry, &info))
    })
    .await
}

/// Metadata of a file the user opened.
#[tauri::command]
pub async fn read_metadata(app: AppHandle, path: PathBuf) -> Result<MetadataReport> {
    app.state::<AppState>().ensure_file(&path)?;
    blocking(move || {
        let info = detect::inspect(&path)?;
        if let Some(p) = info.problem {
            return Err(p);
        }
        let format = info
            .format
            .ok_or_else(|| RadialError::new(ErrorKind::UnsupportedFormat, "Unsupported file."))?;
        if format == Format::Pdf {
            radial_core::pdf::read_metadata(&path)
        } else {
            metadata::read_image(&path, format)
        }
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSuggestion {
    dest_dir: PathBuf,
    stem: String,
    file_name: String,
}

/// Default destination folder and a collision-free file name.
#[tauri::command]
pub fn suggest_output(
    state: State<'_, AppState>,
    source: PathBuf,
    operation: OperationId,
    target: Format,
    dest_dir: Option<PathBuf>,
    stem: Option<String>,
) -> Result<OutputSuggestion> {
    state.ensure_file(&source)?;
    let dir = match dest_dir {
        Some(d) => d,
        None => lock(&state.settings)
            .default_destination
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| source.parent().map(PathBuf::from))
            .ok_or_else(|| internal("Source has no parent folder"))?,
    };
    state.ensure_dir(&dir)?;
    let base = match stem {
        Some(s) => naming::validate_stem(&s)?,
        None => ops::default_stem(operation, &source),
    };
    let path = naming::unique_path(&dir, &base, target, Some(&source));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(base);
    Ok(OutputSuggestion {
        file_name: naming::file_name(&stem, target),
        dest_dir: dir,
        stem,
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobEvent<'a> {
    job_id: &'a str,
    stage: &'a str,
}

fn emit_stage(app: &AppHandle, job_id: &str, stage: &str) {
    let _ = app.emit("radial://job", JobEvent { job_id, stage });
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn run_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    request: OperationRequest,
) -> Result<OperationOutcome> {
    state.ensure_file(&request.source)?;
    state.ensure_dir(&request.dest_dir)?;

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = lock(&state.jobs);
        if jobs.contains_key(&job_id) {
            return Err(RadialError::new(
                ErrorKind::InvalidOptions,
                "That job is already running.",
            ));
        }
        jobs.insert(job_id.clone(), cancel.clone());
    }

    emit_stage(&app, &job_id, "queued");
    let result = match state.slots.acquire().await {
        Err(_) => Err(internal("Job queue closed")),
        Ok(_permit) if cancel.load(Ordering::Relaxed) => Err(RadialError::cancelled()),
        Ok(_permit) => {
            let (app2, id2, req2, cancel2) =
                (app.clone(), job_id.clone(), request.clone(), cancel.clone());
            blocking(move || {
                let state = app2.state::<AppState>();
                ops::run(&state.registry, &req2, &cancel2, |stage| {
                    let s = match stage {
                        Stage::Decoding => "decoding",
                        Stage::Processing => "processing",
                        Stage::Encoding => "encoding",
                        Stage::Verifying => "verifying",
                    };
                    emit_stage(&app2, &id2, s);
                })
            })
            .await
        }
    };
    lock(&state.jobs).remove(&job_id);

    if let Ok(out) = &result {
        lock(&state.access).grant_file(&out.output_path);
    }
    record_history(&state, &job_id, &request, &result);
    result
}

fn record_history(
    state: &AppState,
    job_id: &str,
    req: &OperationRequest,
    result: &Result<OperationOutcome>,
) {
    if !lock(&state.settings).keep_history {
        return;
    }
    // Failures before the source format is known (e.g. not an image) aren't
    // conversions worth listing.
    let source_format = match result {
        Ok(o) => o.source_format,
        Err(_) => match detect::inspect(&req.source).ok().and_then(|i| i.format) {
            Some(f) => f,
            None => return,
        },
    };
    let (status, output_path, error_kind, error_message) = match result {
        Ok(o) => (Status::Succeeded, Some(o.output_path.clone()), None, None),
        Err(e) if e.kind == ErrorKind::Cancelled => (Status::Cancelled, None, Some(e.kind), None),
        Err(e) => (Status::Failed, None, Some(e.kind), Some(e.message.clone())),
    };
    let target_format = match result {
        Ok(o) => o.format,
        Err(_) => req.target.unwrap_or(source_format),
    };
    // Never store user content (annotation text, redaction areas, metadata
    // values) in history; those operations are retried from scratch.
    let options = if req.operation.is_sensitive() {
        ConvertOptions::default()
    } else {
        ConvertOptions {
            annotations: Vec::new(),
            redactions: Vec::new(),
            ..req.options.clone()
        }
    };
    let entry = Entry {
        id: job_id.to_string(),
        source_name: req
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        source_path: req.source.clone(),
        source_format,
        operation: req.operation,
        target_format,
        timestamp: now_ms(),
        status,
        output_path,
        error_kind,
        error_message,
        dest_dir: req.dest_dir.clone(),
        stem: req.stem.clone(),
        options,
    };
    // History is best-effort; a full disk must not turn a success into an error.
    let _ = lock(&state.history).push(entry);
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> bool {
    match lock(&state.jobs).get(&job_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

const PREVIEW_CACHE: usize = 6;

/// Render a preview (downscaled to `max_dim`, with `edits` applied) as raw
/// JPEG/PNG bytes. For PDFs, `page` (0-based) selects the page. Decoded
/// bases are cached so slider changes are fast.
#[tauri::command]
pub async fn preview(
    app: AppHandle,
    path: PathBuf,
    edits: Edits,
    max_dim: u32,
    page: Option<u32>,
) -> Result<Response> {
    let page = page.unwrap_or(0);
    {
        app.state::<AppState>().ensure_file(&path)?;
    }
    edits_valid(&edits)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let cached = lock(&state.previews)
            .iter()
            .find(|e| {
                e.path == path && e.max_dim == max_dim && e.page == page && e.modified == modified
            })
            .map(|e| e.image.clone());
        let base = match cached {
            Some(b) => b,
            None => {
                let b = Arc::new(engine::preview_base(&path, page, max_dim)?);
                let mut cache = lock(&state.previews);
                cache.retain(|e| !(e.path == path && e.max_dim == max_dim && e.page == page));
                cache.insert(
                    0,
                    PreviewEntry {
                        path: path.clone(),
                        modified,
                        max_dim,
                        page,
                        image: b.clone(),
                    },
                );
                cache.truncate(PREVIEW_CACHE);
                b
            }
        };
        Ok(Response::new(engine::render_preview(&base, &edits)?))
    })
    .await
}

fn edits_valid(edits: &Edits) -> Result<()> {
    ConvertOptions {
        edits: *edits,
        ..Default::default()
    }
    .validate(Format::Png)
    .map(|_| ())
}

#[tauri::command]
pub fn history_list(state: State<'_, AppState>) -> Vec<Entry> {
    lock(&state.history).entries().to_vec()
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> Result<()> {
    lock(&state.history).clear()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPlan {
    file: ImportResult,
    operation: OperationId,
    target: Format,
    dest_dir: PathBuf,
    stem: String,
    options: ConvertOptions,
}

/// Re-open the source of a history entry so it can be converted again.
#[tauri::command]
pub async fn history_retry(app: AppHandle, id: String) -> Result<RetryPlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let entry = lock(&state.history).get(&id).cloned().ok_or_else(|| {
            RadialError::new(ErrorKind::NotFound, "That history entry no longer exists.")
        })?;
        let file = state
            .import(std::slice::from_ref(&entry.source_path))
            .pop()
            .ok_or_else(|| internal("import returned nothing"))?;
        if entry.dest_dir.is_dir() {
            lock(&state.access).grant_dir(&entry.dest_dir);
        }
        Ok(RetryPlan {
            file,
            operation: entry.operation,
            target: entry.target_format,
            dest_dir: entry.dest_dir,
            stem: entry.stem,
            options: entry.options,
        })
    })
    .await
}

#[tauri::command]
pub fn open_path(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> Result<()> {
    state.ensure_openable(&path)?;
    if !path.exists() {
        return Err(RadialError::new(
            ErrorKind::NotFound,
            "The file has been moved or deleted.",
        ));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| internal(format!("Could not open the file: {e}")))
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, state: State<'_, AppState>, path: PathBuf) -> Result<()> {
    state.ensure_openable(&path)?;
    if !path.exists() {
        return Err(RadialError::new(
            ErrorKind::NotFound,
            "The file has been moved or deleted.",
        ));
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| internal(format!("Could not show the file: {e}")))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    lock(&state.settings).clone()
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    if let Some(dir) = &settings.default_destination {
        state.ensure_dir(dir)?;
    }
    let mut clean = settings;
    clean.jpeg_quality = clean.jpeg_quality.clamp(1, 100);
    clean.webp_quality = clean.webp_quality.clamp(1, 100);
    if radial_core::options::Rgb::parse(&clean.background).is_none() {
        return Err(RadialError::new(
            ErrorKind::InvalidOptions,
            "Background colour must look like #rrggbb.",
        ));
    }
    clean.save(&state.settings_path)?;
    *lock(&state.settings) = clean.clone();
    Ok(clean)
}
