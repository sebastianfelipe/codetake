//! What the current platform can record.
//!
//! The UI uses this to disable options that are not implemented or not
//! available on the user's OS, and to explain why.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Support {
    pub supported: bool,
    /// Why the feature is unavailable, shown in the UI.
    pub reason: Option<String>,
}

impl Support {
    pub fn yes() -> Self {
        Self {
            supported: true,
            reason: None,
        }
    }

    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            supported: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    /// `macos`, `windows` or `linux`.
    pub os: String,
    pub os_version: String,
    /// Whether recording works at all on this platform.
    pub recording: Support,
    pub display_capture: Support,
    pub window_capture: Support,
    pub camera: Support,
    pub microphone: Support,
    pub system_audio: Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl OsVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn display(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// ScreenCaptureKit with audio capture requires macOS 13 Ventura.
pub const MACOS_MINIMUM: OsVersion = OsVersion::new(13, 0, 0);

pub fn macos_capabilities(version: OsVersion) -> PlatformCapabilities {
    let base = PlatformCapabilities {
        os: "macos".into(),
        os_version: version.display(),
        recording: Support::yes(),
        display_capture: Support::yes(),
        window_capture: Support::yes(),
        camera: Support::yes(),
        microphone: Support::yes(),
        system_audio: Support::yes(),
    };
    if version >= MACOS_MINIMUM {
        return base;
    }
    let reason = format!(
        "CodeTake requires macOS {} or later (this Mac runs {}).",
        MACOS_MINIMUM.display(),
        version.display()
    );
    PlatformCapabilities {
        recording: Support::no(&reason),
        display_capture: Support::no(&reason),
        window_capture: Support::no(&reason),
        system_audio: Support::no(&reason),
        ..base
    }
}

/// Capabilities for platforms whose capture backend is not implemented yet.
///
/// CodeTake does not pretend to record on these platforms: the UI shows the
/// reason and the Record button stays disabled.
pub fn unimplemented_platform_capabilities(os: &str, os_version: String) -> PlatformCapabilities {
    let backend = match os {
        "windows" => "Windows.Graphics.Capture and Media Foundation",
        "linux" => "XDG Desktop Portal and PipeWire",
        _ => "a native capture backend",
    };
    let reason = format!(
        "Recording on {os} is not implemented yet. The {backend} backend is on the roadmap; \
         contributions are welcome."
    );
    PlatformCapabilities {
        os: os.into(),
        os_version,
        recording: Support::no(&reason),
        display_capture: Support::no(&reason),
        window_capture: Support::no(&reason),
        camera: Support::no(&reason),
        microphone: Support::no(&reason),
        system_audio: Support::no(&reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_macos_supports_everything() {
        let caps = macos_capabilities(OsVersion::new(15, 2, 0));
        assert_eq!(caps.os, "macos");
        assert_eq!(caps.os_version, "15.2.0");
        for support in [
            &caps.recording,
            &caps.display_capture,
            &caps.window_capture,
            &caps.camera,
            &caps.microphone,
            &caps.system_audio,
        ] {
            assert!(support.supported);
        }
    }

    #[test]
    fn macos_13_is_the_minimum() {
        assert!(
            macos_capabilities(OsVersion::new(13, 0, 0))
                .recording
                .supported
        );
        let old = macos_capabilities(OsVersion::new(12, 6, 1));
        assert!(!old.recording.supported);
        assert!(!old.system_audio.supported);
        assert!(old.recording.reason.unwrap().contains("macOS 13.0.0"));
    }

    #[test]
    fn unimplemented_platforms_explain_why() {
        let caps = unimplemented_platform_capabilities("linux", "6.8".into());
        assert!(!caps.recording.supported);
        assert!(caps.recording.reason.unwrap().contains("PipeWire"));
        let caps = unimplemented_platform_capabilities("windows", "11".into());
        assert!(caps
            .system_audio
            .reason
            .unwrap()
            .contains("Media Foundation"));
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(OsVersion::new(13, 10, 0) > OsVersion::new(13, 9, 5));
        assert!(OsVersion::new(14, 0, 0) > OsVersion::new(13, 99, 99));
    }
}
