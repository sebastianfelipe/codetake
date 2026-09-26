pub mod audio;
pub mod capabilities;
pub mod commands;
pub mod config;
pub mod devices;
pub mod error;
pub mod music;
pub mod output;
pub mod permissions;
pub mod platform;
pub mod recording;
pub mod video;

use tauri::Emitter;

/// Emitted when the global record shortcut is pressed; the UI decides
/// whether that starts or stops a recording.
pub const EVENT_TOGGLE_SHORTCUT: &str = "shortcut://toggle-recording";
pub const RECORD_SHORTCUT: &str = "CommandOrControl+Shift+R";

fn register_shortcut(app: &tauri::App) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let result = app
        .global_shortcut()
        .on_shortcut(RECORD_SHORTCUT, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit(EVENT_TOGGLE_SHORTCUT, ());
            }
        });
    if let Err(error) = result {
        // Another app may own the shortcut; recording still works from the UI.
        log::warn!("could not register {RECORD_SHORTCUT}: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::AppState::default())
        .setup(|app| {
            register_shortcut(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_capabilities,
            commands::get_permissions,
            commands::request_permission,
            commands::open_permission_settings,
            commands::list_displays,
            commands::list_windows,
            commands::list_cameras,
            commands::list_microphones,
            commands::available_resolutions,
            commands::list_music_tracks,
            commands::default_output_directory,
            commands::recover_recordings,
            commands::load_settings,
            commands::save_settings,
            commands::start_preview,
            commands::stop_preview,
            commands::start_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the CodeTake application");
}
