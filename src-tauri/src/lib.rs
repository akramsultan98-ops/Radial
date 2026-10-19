//! RADIAL desktop shell: window, native integration and IPC commands.
//! All conversion logic lives in `radial-core`.

mod access;
mod commands;
mod state;

use state::AppState;
use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // The bundled PDFium library ships in the app's resources.
            if let Ok(res) = app.path().resource_dir() {
                radial_core::pdf::configure([res.join("pdfium"), res.join("resources/pdfium")]);
            }
            let data_dir = app.path().app_data_dir()?;
            let state = AppState::new(&data_dir);
            // Files passed on the command line ("Open with RADIAL").
            let launch: Vec<_> = std::env::args_os()
                .skip(1)
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute() && p.is_file())
                .collect();
            state.set_launch_files(launch);
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                let app = window.app_handle().clone();
                let paths = paths.clone();
                // Inspect off the event loop; header parsing touches the disk.
                tauri::async_runtime::spawn_blocking(move || {
                    let state = app.state::<AppState>();
                    let files = state.import(&paths);
                    let _ = app.emit("radial://files-dropped", files);
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::take_launch_files,
            commands::pick_files,
            commands::pick_directory,
            commands::source_formats,
            commands::operations_for,
            commands::read_metadata,
            commands::suggest_output,
            commands::run_operation,
            commands::cancel_job,
            commands::preview,
            commands::history_list,
            commands::history_clear,
            commands::history_retry,
            commands::open_path,
            commands::reveal_path,
            commands::get_settings,
            commands::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RADIAL");
}
