//! The menu bar (system tray) icon: shows what will be recorded and the
//! running timer, and starts, pauses and stops recordings without opening
//! the main window.
//!
//! The UI owns the recording flow (countdown, config validation), so the
//! tray only renders the state the UI sends with [`update_tray`] and
//! forwards menu clicks back to it as [`EVENT_TRAY_ACTION`] events.

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::error::{AppError, AppResult};

pub const TRAY_ID: &str = "main";
pub const EVENT_TRAY_ACTION: &str = "tray://action";

const ICON: &[u8] = include_bytes!("../icons/tray/tray-template@2x.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayPhase {
    #[default]
    Idle,
    Countdown,
    Starting,
    Recording,
    Paused,
    Saving,
}

/// What the tray menu shows; sent by the UI whenever it changes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayState {
    pub phase: TrayPhase,
    /// Whether "Start Recording" is available (the setup is valid).
    pub can_record: bool,
    /// Why recording is unavailable, shown instead of "Ready to record".
    pub blocked_reason: Option<String>,
    /// One line per setting: "Screen: Built-in Display", "Camera: Off", ...
    pub details: Vec<String>,
}

/// Menu actions forwarded to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayAction {
    Record,
    Stop,
    Pause,
    Resume,
    CancelCountdown,
    ShowFolder,
}

impl TrayAction {
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "record" => TrayAction::Record,
            "stop" => TrayAction::Stop,
            "pause" => TrayAction::Pause,
            "resume" => TrayAction::Resume,
            "cancel" => TrayAction::CancelCountdown,
            "folder" => TrayAction::ShowFolder,
            _ => return None,
        })
    }
}

/// Menu entries for a state, as (id, label, enabled). `None` is a separator.
/// Kept separate from the Tauri menu types so it can be unit tested.
pub fn menu_entries(state: &TrayState) -> Vec<Option<(&'static str, String, bool)>> {
    let status = match state.phase {
        TrayPhase::Idle if state.can_record => "Ready to record".to_string(),
        TrayPhase::Idle => state
            .blocked_reason
            .clone()
            .unwrap_or_else(|| "Not ready to record".into()),
        TrayPhase::Countdown => "Starting soon…".into(),
        TrayPhase::Starting => "Starting…".into(),
        TrayPhase::Recording => "● Recording".into(),
        TrayPhase::Paused => "Paused".into(),
        TrayPhase::Saving => "Saving the recording…".into(),
    };

    let mut entries = vec![Some(("status", status, false)), None];
    entries.extend(
        state
            .details
            .iter()
            .map(|line| Some(("detail", line.clone(), false))),
    );
    if !state.details.is_empty() {
        entries.push(None);
    }

    match state.phase {
        TrayPhase::Idle => {
            entries.push(Some(("record", "Start Recording".into(), state.can_record)))
        }
        TrayPhase::Countdown => entries.push(Some(("cancel", "Cancel Countdown".into(), true))),
        TrayPhase::Starting | TrayPhase::Saving => {
            entries.push(Some(("stop", "Stop Recording".into(), false)))
        }
        TrayPhase::Recording => {
            entries.push(Some(("stop", "Stop Recording".into(), true)));
            entries.push(Some(("pause", "Pause".into(), true)));
        }
        TrayPhase::Paused => {
            entries.push(Some(("stop", "Stop Recording".into(), true)));
            entries.push(Some(("resume", "Resume".into(), true)));
        }
    }
    entries.push(None);
    entries.push(Some(("show", "Open CodeTake…".into(), true)));
    entries.push(Some(("folder", "Show Recordings Folder".into(), true)));
    entries.push(None);
    entries.push(Some(("quit", "Quit CodeTake".into(), true)));
    entries
}

fn build_menu(app: &AppHandle, state: &TrayState) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app);
    for (index, entry) in menu_entries(state).into_iter().enumerate() {
        builder = match entry {
            None => builder.separator(),
            Some((id, label, enabled)) => {
                // Informational rows share an id prefix; ids must be unique.
                let id = if id == "detail" {
                    format!("detail-{index}")
                } else {
                    id.to_string()
                };
                let mut item = MenuItemBuilder::with_id(id.clone(), label).enabled(enabled);
                if id == "record" || id == "stop" {
                    item = item.accelerator("CmdOrCtrl+Shift+R");
                }
                builder.item(&item.build(app)?)
            }
        };
    }
    builder.build()
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &TrayState::default())?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON)?)
        .icon_as_template(true)
        .tooltip("CodeTake")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => show_main_window(app),
                "quit" => crate::quit(app),
                _ => {
                    if let Some(action) = TrayAction::from_id(id) {
                        let _ = app.emit(EVENT_TRAY_ACTION, action);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn tray_error(error: tauri::Error) -> AppError {
    AppError::InvalidState(format!("menu bar icon: {error}"))
}

#[tauri::command]
pub fn update_tray(app: AppHandle, state: TrayState) -> AppResult<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let menu = build_menu(&app, &state).map_err(tray_error)?;
    tray.set_menu(Some(menu)).map_err(tray_error)
}

/// Text next to the menu bar icon (macOS), e.g. the recording timer.
#[tauri::command]
pub fn set_tray_title(app: AppHandle, title: Option<String>) -> AppResult<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    tray.set_title(title.as_deref()).map_err(tray_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(state: &TrayState) -> Vec<(&'static str, bool)> {
        menu_entries(state)
            .into_iter()
            .flatten()
            .filter(|(id, _, _)| *id != "detail" && *id != "status")
            .map(|(id, _, enabled)| (id, enabled))
            .collect()
    }

    fn status(state: &TrayState) -> String {
        menu_entries(state)[0].clone().unwrap().1
    }

    #[test]
    fn idle_offers_start_only_when_the_setup_is_valid() {
        let ready = TrayState {
            can_record: true,
            ..TrayState::default()
        };
        assert_eq!(ids(&ready)[0], ("record", true));
        assert_eq!(status(&ready), "Ready to record");

        let blocked = TrayState {
            can_record: false,
            blocked_reason: Some("Camera permission is needed".into()),
            ..TrayState::default()
        };
        assert_eq!(ids(&blocked)[0], ("record", false));
        assert_eq!(status(&blocked), "Camera permission is needed");
    }

    #[test]
    fn recording_offers_stop_and_pause_and_paused_offers_resume() {
        let recording = TrayState {
            phase: TrayPhase::Recording,
            ..TrayState::default()
        };
        assert_eq!(&ids(&recording)[..2], &[("stop", true), ("pause", true)]);
        let paused = TrayState {
            phase: TrayPhase::Paused,
            ..TrayState::default()
        };
        assert_eq!(&ids(&paused)[..2], &[("stop", true), ("resume", true)]);
    }

    #[test]
    fn countdown_can_be_cancelled_and_saving_cannot_be_interrupted() {
        let countdown = TrayState {
            phase: TrayPhase::Countdown,
            ..TrayState::default()
        };
        assert_eq!(ids(&countdown)[0], ("cancel", true));
        let saving = TrayState {
            phase: TrayPhase::Saving,
            ..TrayState::default()
        };
        assert_eq!(ids(&saving)[0], ("stop", false));
    }

    #[test]
    fn shows_the_current_setup_and_always_offers_open_and_quit() {
        let state = TrayState {
            can_record: true,
            details: vec!["Screen: Built-in Display".into(), "Camera: Off".into()],
            ..TrayState::default()
        };
        let labels: Vec<String> = menu_entries(&state)
            .into_iter()
            .flatten()
            .map(|(_, label, _)| label)
            .collect();
        assert!(labels.contains(&"Screen: Built-in Display".to_string()));
        assert!(labels.contains(&"Camera: Off".to_string()));
        let all = ids(&state);
        assert!(all.contains(&("show", true)));
        assert!(all.contains(&("folder", true)));
        assert_eq!(all.last(), Some(&("quit", true)));
    }

    #[test]
    fn maps_menu_ids_to_actions() {
        assert_eq!(TrayAction::from_id("record"), Some(TrayAction::Record));
        assert_eq!(
            TrayAction::from_id("cancel"),
            Some(TrayAction::CancelCountdown)
        );
        assert_eq!(TrayAction::from_id("detail-3"), None);
    }
}
