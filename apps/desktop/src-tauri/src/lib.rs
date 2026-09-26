pub mod audio;
pub mod config;
pub mod error;
pub mod output;
pub mod permissions;
pub mod recording;
pub mod video;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to start the CodeTake application");
}
