use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::permissions::PermissionKind;

pub type AppResult<T> = Result<T, AppError>;

/// Every failure CodeTake can report to the user.
///
/// Errors are serialized to the frontend as `{ code, message }` so the UI can
/// react to specific failures (for example by showing a permission prompt)
/// while still displaying a human readable message.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} permission has not been granted")]
    PermissionDenied(PermissionKind),

    #[error("{0}")]
    Unsupported(String),

    #[error("Invalid recording configuration: {0}")]
    InvalidConfig(String),

    #[error("Device unavailable: {0}")]
    DeviceUnavailable(String),

    #[error("Screen capture failed: {0}")]
    ScreenCapture(String),

    #[error("Camera capture failed: {0}")]
    Camera(String),

    #[error("Microphone capture failed: {0}")]
    Microphone(String),

    #[error("System audio capture failed: {0}")]
    SystemAudio(String),

    #[error("Video encoder failed: {0}")]
    Encoder(String),

    #[error("Could not load background music: {0}")]
    Music(String),

    #[error("Output location problem: {0}")]
    Storage(String),

    #[error("{0}")]
    InvalidState(String),

    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
}

impl AppError {
    /// Stable machine-readable identifier used by the frontend.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::PermissionDenied(_) => "permission_denied",
            AppError::Unsupported(_) => "unsupported",
            AppError::InvalidConfig(_) => "invalid_config",
            AppError::DeviceUnavailable(_) => "device_unavailable",
            AppError::ScreenCapture(_) => "screen_capture",
            AppError::Camera(_) => "camera",
            AppError::Microphone(_) => "microphone",
            AppError::SystemAudio(_) => "system_audio",
            AppError::Encoder(_) => "encoder",
            AppError::Music(_) => "music",
            AppError::Storage(_) => "storage",
            AppError::InvalidState(_) => "invalid_state",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 3)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        let permission = match self {
            AppError::PermissionDenied(kind) => Some(kind),
            _ => None,
        };
        state.serialize_field("permission", &permission)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_message() {
        let error = AppError::Encoder("disk full".into());
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "encoder");
        assert_eq!(json["message"], "Video encoder failed: disk full");
        assert!(json["permission"].is_null());
    }

    #[test]
    fn permission_errors_name_the_permission() {
        let error = AppError::PermissionDenied(PermissionKind::ScreenRecording);
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "permission_denied");
        assert_eq!(json["permission"], "screenRecording");
    }
}
