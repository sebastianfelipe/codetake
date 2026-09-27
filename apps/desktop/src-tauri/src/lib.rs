pub mod audio;
pub mod capabilities;
pub mod commands;
pub mod config;
pub mod devices;
pub mod error;
pub mod export;
pub mod music;
pub mod output;
pub mod permissions;
pub mod platform;
pub mod recording;
pub mod tray;
pub mod video;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

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

/// Quits the app, finalizing a running recording first so it is never lost.
pub fn quit(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        commands::finish_active_recording(&app);
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Warnings and errors go to stderr (RUST_LOG=debug for more detail).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::AppState::default())
        .setup(|app| {
            register_shortcut(app);
            if let Err(error) = tray::create(app.handle()) {
                log::warn!("could not create the menu bar icon: {error}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps CodeTake running in the menu bar,
            // so a recording can continue and be controlled from there.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.app_handle().tray_by_id(tray::TRAY_ID).is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                    commands::release_preview(window.app_handle());
                }
            }
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
            commands::allow_media,
            commands::music_track_file,
            commands::export_video,
            tray::update_tray,
            tray::set_tray_indicator,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start the CodeTake application");

    app.run(|app, event| match event {
        // Cmd+Q or quitting from the Dock while recording: finalize first.
        RunEvent::ExitRequested { api, code, .. }
            if code.is_none() && commands::is_recording(app) =>
        {
            api.prevent_exit();
            quit(app);
        }
        // Clicking the Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => tray::show_main_window(app),
        _ => {}
    });
}
