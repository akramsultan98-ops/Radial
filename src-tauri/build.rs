fn main() {
    // Declare app commands explicitly so each must be granted in a capability
    // file (see capabilities/default.json) instead of being implicitly allowed.
    let commands = &[
        "take_launch_files",
        "pick_files",
        "pick_directory",
        "source_formats",
        "operations_for",
        "read_metadata",
        "suggest_output",
        "run_operation",
        "cancel_job",
        "preview",
        "history_list",
        "history_clear",
        "history_retry",
        "open_path",
        "reveal_path",
        "get_settings",
        "set_settings",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}
