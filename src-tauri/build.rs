const COMMANDS: &[&str] = &[
    "app_info",
    "app_check_update",
    "app_install_update",
    "detect_url",
    "analyze_url",
    "cancel_analysis",
    "cache_thumbnail",
    "plan_download",
    "enqueue_downloads",
    "queue_list",
    "queue_cancel",
    "queue_pause",
    "queue_resume",
    "queue_retry",
    "queue_remove",
    "queue_clear_finished",
    "queue_move",
    "queue_pause_all",
    "queue_resume_all",
    "queue_reveal",
    "queue_open",
    "history_query",
    "history_stats",
    "history_find_completed",
    "history_open",
    "history_reveal",
    "history_redownload",
    "history_delete",
    "history_clear",
    "open_download_folder",
    "settings_get",
    "settings_update",
    "settings_reset",
    "deps_status",
    "deps_refresh",
    "deps_check_updates",
    "deps_install",
    "deps_diagnostics",
    "clear_temporary_files",
    "validate_directory",
    "has_active_downloads",
];

fn main() {
    println!("cargo:rerun-if-env-changed=DARKLOAD_REPOSITORY");
    // Declaring the command list makes every IPC command opt-in through capabilities.
    let mut attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // The Windows application manifest is embedded into every linked target (application,
        // tests and examples) so unit tests can load Common Controls v6 like the application.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_windows_manifest();
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

fn embed_windows_manifest() {
    let manifest = std::env::current_dir()
        .expect("current dir")
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        return;
    }
    // MinGW toolchains: compile a resource script with windres and link the object everywhere.
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out_dir.join("darkload-manifest.rc");
    let object = out_dir.join("darkload-manifest.o");
    let manifest_path = manifest.display().to_string().replace('\\', "/");
    std::fs::write(&rc, format!("1 24 \"{manifest_path}\"\n")).expect("write resource script");
    let windres = ["x86_64-w64-mingw32-windres", "windres"]
        .into_iter()
        .find(|tool| {
            std::process::Command::new(tool)
                .arg("--version")
                .output()
                .is_ok()
        })
        .expect("windres is required to build DarkLoad with a GNU toolchain");
    let status = std::process::Command::new(windres)
        .arg(&rc)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&object)
        .status()
        .expect("run windres");
    assert!(status.success(), "windres failed");
    println!("cargo:rustc-link-arg={}", object.display());
}
