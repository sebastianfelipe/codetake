use std::fmt;

use serde::{Deserialize, Serialize};

/// A system permission CodeTake may need before it can record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    ScreenRecording,
    Camera,
    Microphone,
}

impl fmt::Display for PermissionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            PermissionKind::ScreenRecording => "Screen Recording",
            PermissionKind::Camera => "Camera",
            PermissionKind::Microphone => "Microphone",
        };
        f.write_str(name)
    }
}

/// The authorization state of a permission, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    /// The user has granted access.
    Granted,
    /// The user has explicitly denied access; only System Settings can change it.
    Denied,
    /// Access is blocked by policy (for example parental controls or MDM).
    Restricted,
    /// The user has not been asked yet.
    NotDetermined,
    /// The platform does not gate this resource behind a permission.
    NotRequired,
}

impl PermissionStatus {
    pub fn is_usable(self) -> bool {
        matches!(
            self,
            PermissionStatus::Granted | PermissionStatus::NotRequired
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionState {
    pub screen_recording: PermissionStatus,
    pub camera: PermissionStatus,
    pub microphone: PermissionStatus,
}

impl PermissionState {
    pub fn status(&self, kind: PermissionKind) -> PermissionStatus {
        match kind {
            PermissionKind::ScreenRecording => self.screen_recording,
            PermissionKind::Camera => self.camera,
            PermissionKind::Microphone => self.microphone,
        }
    }
}
