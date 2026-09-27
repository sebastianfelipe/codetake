//! The menu bar (system tray) icon: shows what will be recorded and the
//! running timer, and starts, pauses and stops recordings without opening
//! the main window.
//!
//! The UI owns the recording flow (countdown, config validation), so the
//! tray only renders the state the UI sends with [`update_tray`] and
//! forwards menu clicks back to it as [`EVENT_TRAY_ACTION`] events.

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::error::{AppError, AppResult};

pub const TRAY_ID: &str = "main";
pub const EVENT_TRAY_ACTION: &str = "tray://action";

/// The CodeTake glyph, as a template image that adapts to the menu bar.
const ICON: &[u8] = include_bytes!("../icons/tray/tray-template@2x.png");
/// Shown while recording: a red stop square, readable on any menu bar.
const RECORDING_ICON: &[u8] = include_bytes!("../icons/tray/tray-recording@2x.png");

/// The menu's status line, updated in place every second while recording
/// (rebuilding the menu would close it if it is open), and the state it
/// was built from.
static STATUS_ITEM: std::sync::Mutex<Option<MenuItem<Wry>>> = std::sync::Mutex::new(None);
static LAST_STATE: std::sync::Mutex<Option<TrayState>> = std::sync::Mutex::new(None);

/// Whether the recording icon is currently shown (avoids re-setting it).
static SHOWING_RECORDING_ICON: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
    /// Recorded time shown in the menu while recording, e.g. "12:42".
    #[serde(default)]
    pub elapsed: Option<String>,
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
/// The first, informational line of the menu.
pub fn status_text(state: &TrayState) -> String {
    let with_time = |label: &str| match &state.elapsed {
        Some(elapsed) => format!("{label} — {elapsed}"),
        None => label.to_string(),
    };
    match state.phase {
        TrayPhase::Idle if state.can_record => "Ready to record".to_string(),
        TrayPhase::Idle => state
            .blocked_reason
            .clone()
            .unwrap_or_else(|| "Not ready to record".into()),
        TrayPhase::Countdown => "Starting soon…".into(),
        TrayPhase::Starting => "Starting…".into(),
        TrayPhase::Recording => with_time("● Recording"),
        TrayPhase::Paused => with_time("Paused"),
        TrayPhase::Saving => "Saving the recording…".into(),
    }
}

pub fn menu_entries(state: &TrayState) -> Vec<Option<(&'static str, String, bool)>> {
    let status = status_text(state);
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
                let item = item.build(app)?;
                if id == "status" {
                    if let Ok(mut status) = STATUS_ITEM.lock() {
                        *status = Some(item.clone());
                    }
                }
                builder.item(&item)
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
    tray.set_menu(Some(menu)).map_err(tray_error)?;
    if let Ok(mut last) = LAST_STATE.lock() {
        *last = Some(state);
    }
    Ok(())
}

/// Updates the menu bar indicator: the red stop icon while `recording`,
/// optional text next to it (macOS, e.g. a compact timer), and the recorded
/// time on the menu's status line.
#[tauri::command]
pub fn set_tray_indicator(
    app: AppHandle,
    recording: bool,
    title: Option<String>,
    elapsed: Option<String>,
) -> AppResult<()> {
    use std::sync::atomic::Ordering;

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    if SHOWING_RECORDING_ICON.swap(recording, Ordering::Relaxed) != recording {
        let icon =
            Image::from_bytes(if recording { RECORDING_ICON } else { ICON }).map_err(tray_error)?;
        tray.set_icon(Some(icon)).map_err(tray_error)?;
        tray.set_icon_as_template(!recording).map_err(tray_error)?;
    }
    if let (Ok(mut last), Ok(status)) = (LAST_STATE.lock(), STATUS_ITEM.lock()) {
        if let (Some(state), Some(item)) = (last.as_mut(), status.as_ref()) {
            if state.elapsed != elapsed {
                state.elapsed = elapsed;
                let _ = item.set_text(status_text(state));
            }
        }
    }
    // On macOS `set_title(None)` leaves the previous text in place; an empty
    // string actually clears it.
    tray.set_title(Some(title.as_deref().unwrap_or("")))
        .map_err(tray_error)
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
        let text = menu_entries(state)[0].clone().unwrap().1;
        assert_eq!(text, status_text(state));
        text
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
    fn shows_the_recorded_time_in_the_menu() {
        let state = TrayState {
            phase: TrayPhase::Recording,
            elapsed: Some("12:42".into()),
            ..TrayState::default()
        };
        assert_eq!(status(&state), "● Recording — 12:42");
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
