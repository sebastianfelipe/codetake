//! Where recordings are written and how they are named.
//!
//! Recordings are organized in one folder per day:
//!
//! ```text
//! CodeTake/
//! └── 2026-09-26/
//!     └── coding-session-2026-09-26-09-32-14.mp4
//! ```
//!
//! While a recording is in progress it is written to a `.partial.<ext>` file
//! next to its final location. The intermediate container is chosen by the
//! platform backend (on macOS a crash-safe fragmented QuickTime movie) and is
//! turned into the final MP4 when recording stops. If CodeTake exits
//! unexpectedly, the partial file is left behind and is recovered (renamed)
//! on the next launch instead of being deleted.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::error::{AppError, AppResult};

const FILE_PREFIX: &str = "coding-session";
const PARTIAL_MARKER: &str = ".partial.";
const RECOVERED_SUFFIX: &str = "-recovered";
const INCOMPLETE_SUFFIX: &str = "-incomplete";

/// Recording won't start with less free space than this.
pub const MIN_FREE_BYTES_TO_START: u64 = 1024 * 1024 * 1024;
/// A running recording is stopped and finalized below this much free space.
pub const MIN_FREE_BYTES_WHILE_RECORDING: u64 = 256 * 1024 * 1024;

/// The default output directory, `<videos>/CodeTake` (on macOS `~/Movies/CodeTake`).
pub fn default_output_directory(videos_dir: &Path) -> PathBuf {
    videos_dir.join("CodeTake")
}

pub fn date_folder_name(time: &DateTime<Local>) -> String {
    time.format("%Y-%m-%d").to_string()
}

pub fn recording_file_stem(time: &DateTime<Local>) -> String {
    format!("{FILE_PREFIX}-{}", time.format("%Y-%m-%d-%H-%M-%S"))
}

/// The files used by a single recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPlan {
    /// Where the finished MP4 ends up.
    pub final_path: PathBuf,
    /// Where media is written while recording.
    pub partial_path: PathBuf,
}

impl OutputPlan {
    fn stem(&self) -> String {
        self.final_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn intermediate_extension(&self) -> String {
        self.partial_path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mp4".into())
    }

    /// Path used when a recording could not be finalized cleanly but media
    /// was written; the file is kept rather than deleted.
    pub fn incomplete_path(&self) -> PathBuf {
        self.final_path.with_file_name(format!(
            "{}{INCOMPLETE_SUFFIX}.{}",
            self.stem(),
            self.intermediate_extension()
        ))
    }

    /// Path used when the recording is complete but could not be converted
    /// to the final container (e.g. `coding-session-….mov`).
    pub fn fallback_path(&self) -> PathBuf {
        self.final_path
            .with_extension(self.intermediate_extension())
    }
}

/// Builds the output paths for a recording started at `time`, written
/// through an intermediate file with the extension `intermediate`.
///
/// Does not touch the file system; if the name is already taken (two
/// recordings in the same second), `exists` lets the caller add a counter.
pub fn plan_output(
    base: &Path,
    time: &DateTime<Local>,
    intermediate: &str,
    exists: impl Fn(&Path) -> bool,
) -> OutputPlan {
    let folder = base.join(date_folder_name(time));
    let stem = recording_file_stem(time);

    let mut counter = 1;
    loop {
        let name = if counter == 1 {
            stem.clone()
        } else {
            format!("{stem}-{counter}")
        };
        let plan = OutputPlan {
            final_path: folder.join(format!("{name}.mp4")),
            partial_path: folder.join(format!("{name}{PARTIAL_MARKER}{intermediate}")),
        };
        let taken = [
            &plan.final_path,
            &plan.partial_path,
            &plan.fallback_path(),
            &plan.incomplete_path(),
        ]
        .into_iter()
        .any(|p| exists(p));
        if !taken {
            return plan;
        }
        counter += 1;
    }
}

/// Name of the folder (inside each day) holding the raw recordings.
pub const RAW_FOLDER: &str = "raw";

/// The files of one recording session: the raw screen and camera
/// recordings, and the final video that the review step exports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingFiles {
    /// `<day>/raw/<name>-screen.mp4` — screen, microphone and system audio.
    pub screen: OutputPlan,
    /// `<day>/raw/<name>-camera.mp4` — the webcam, when enabled.
    pub camera: Option<OutputPlan>,
    /// `<day>/<name>.mp4` — where the finished video is exported.
    pub export_path: PathBuf,
}

/// Plans the files for a recording started at `time`.
pub fn plan_recording(
    base: &Path,
    time: &DateTime<Local>,
    intermediate: &str,
    with_camera: bool,
    exists: impl Fn(&Path) -> bool,
) -> RecordingFiles {
    let day = base.join(date_folder_name(time));
    let raw = day.join(RAW_FOLDER);
    let stem = recording_file_stem(time);
    let file = |name: &str, kind: &str| OutputPlan {
        final_path: raw.join(format!("{name}-{kind}.mp4")),
        partial_path: raw.join(format!("{name}-{kind}{PARTIAL_MARKER}{intermediate}")),
    };

    let mut counter = 1;
    loop {
        let name = if counter == 1 {
            stem.clone()
        } else {
            format!("{stem}-{counter}")
        };
        let files = RecordingFiles {
            screen: file(&name, "screen"),
            camera: with_camera.then(|| file(&name, "camera")),
            export_path: day.join(format!("{name}.mp4")),
        };
        let mut candidates = vec![files.export_path.clone()];
        for plan in std::iter::once(&files.screen).chain(files.camera.iter()) {
            candidates.extend([
                plan.final_path.clone(),
                plan.partial_path.clone(),
                plan.fallback_path(),
                plan.incomplete_path(),
            ]);
        }
        if !candidates.iter().any(|p| exists(p)) {
            return files;
        }
        counter += 1;
    }
}

/// A free file name next to `path`: `name.mp4`, then `name-2.mp4`, ...
pub fn unique_path(path: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    if !exists(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mp4".into());
    (2..)
        .map(|n| path.with_file_name(format!("{stem}-{n}.{extension}")))
        .find(|candidate| !exists(candidate))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Creates the dated folder and checks that it is writable and has room.
pub fn prepare_output(plan: &OutputPlan, free_space: Option<u64>) -> AppResult<()> {
    let folder = plan
        .final_path
        .parent()
        .ok_or_else(|| AppError::Storage("invalid output path".into()))?;

    fs::create_dir_all(folder)
        .map_err(|e| AppError::Storage(format!("cannot create {}: {e}", folder.display())))?;

    let probe = folder.join(".codetake-write-test");
    fs::write(&probe, b"ok")
        .map_err(|e| AppError::Storage(format!("{} is not writable: {e}", folder.display())))?;
    let _ = fs::remove_file(&probe);

    if let Some(free) = free_space {
        if free < MIN_FREE_BYTES_TO_START {
            return Err(AppError::Storage(format!(
                "only {} MB free in {}; at least {} MB is needed to start recording",
                free / (1024 * 1024),
                folder.display(),
                MIN_FREE_BYTES_TO_START / (1024 * 1024)
            )));
        }
    }
    Ok(())
}

/// Keeps a finished recording in its intermediate container when it could
/// not be converted to MP4.
pub fn keep_fallback(plan: &OutputPlan) -> Option<PathBuf> {
    let target = plan.fallback_path();
    fs::rename(&plan.partial_path, &target).ok()?;
    Some(target)
}

/// Keeps a recording that could not be finalized, returning its new path.
pub fn keep_incomplete(plan: &OutputPlan) -> Option<PathBuf> {
    let has_media = fs::metadata(&plan.partial_path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    if !has_media {
        let _ = fs::remove_file(&plan.partial_path);
        return None;
    }
    let target = plan.incomplete_path();
    fs::rename(&plan.partial_path, &target).ok()?;
    Some(target)
}

/// Removes the partial file of a recording that never produced media, or
/// that has been converted to its final file.
pub fn discard(plan: &OutputPlan) {
    let _ = fs::remove_file(&plan.partial_path);
}

/// Finds partial recordings left behind by a crash. Empty partial files
/// (the crash happened before any media was written) are deleted.
pub fn find_partial_recordings(base: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(days) = fs::read_dir(base) else {
        return found;
    };
    let folders = days
        .flatten()
        .flat_map(|day| [day.path(), day.path().join(RAW_FOLDER)]);
    for folder in folders {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with(FILE_PREFIX) || !name.contains(PARTIAL_MARKER) {
                continue;
            }
            if entry.metadata().map(|m| m.len() == 0).unwrap_or(true) {
                let _ = fs::remove_file(&path);
                continue;
            }
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Where a recovered partial recording goes: the MP4 it is converted to,
/// and the name it is kept under if conversion fails.
pub fn recovery_paths(partial: &Path) -> Option<(PathBuf, PathBuf)> {
    let name = partial.file_name()?.to_str()?;
    let (stem, extension) = name.rsplit_once(PARTIAL_MARKER)?;
    Some((
        partial.with_file_name(format!("{stem}{RECOVERED_SUFFIX}.mp4")),
        partial.with_file_name(format!("{stem}{RECOVERED_SUFFIX}.{extension}")),
    ))
}

/// Recovers every partial recording under `base`, converting each with
/// `convert(partial, mp4)` and falling back to renaming it. Returns the
/// recovered files.
pub fn recover_partial_recordings(
    base: &Path,
    convert: impl Fn(&Path, &Path) -> AppResult<()>,
) -> Vec<PathBuf> {
    let mut recovered = Vec::new();
    for partial in find_partial_recordings(base) {
        let Some((mp4, fallback)) = recovery_paths(&partial) else {
            continue;
        };
        if convert(&partial, &mp4).is_ok() {
            let _ = fs::remove_file(&partial);
            recovered.push(mp4);
        } else {
            let _ = fs::remove_file(&mp4);
            if fs::rename(&partial, &fallback).is_ok() {
                recovered.push(fallback);
            }
        }
    }
    recovered
}

/// Free space on the volume containing `path`, if it can be determined.
#[cfg(unix)]
pub fn available_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // Walk up to the nearest existing ancestor (the output folder may not exist yet).
    let existing = path.ancestors().find(|p| p.exists())?;
    let c_path = CString::new(existing.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated string and `stats` is a
    // properly sized buffer that statvfs fills on success.
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: statvfs returned 0, so the struct has been initialized.
    let stats = unsafe { stats.assume_init() };
    #[allow(clippy::unnecessary_cast)]
    Some(stats.f_bavail as u64 * stats.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn available_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn time() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 26, 9, 32, 14).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codetake-test-{name}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_directory_is_codetake_inside_videos() {
        assert_eq!(
            default_output_directory(Path::new("/Users/dev/Movies")),
            PathBuf::from("/Users/dev/Movies/CodeTake")
        );
    }

    #[test]
    fn file_names_encode_date_and_time() {
        assert_eq!(date_folder_name(&time()), "2026-09-26");
        assert_eq!(
            recording_file_stem(&time()),
            "coding-session-2026-09-26-09-32-14"
        );
    }

    #[test]
    fn plans_dated_folder_with_partial_file() {
        let plan = plan_output(Path::new("/rec"), &time(), "mov", |_| false);
        let day = PathBuf::from("/rec/2026-09-26");
        assert_eq!(
            plan.final_path,
            day.join("coding-session-2026-09-26-09-32-14.mp4")
        );
        assert_eq!(
            plan.partial_path,
            day.join("coding-session-2026-09-26-09-32-14.partial.mov")
        );
        assert_eq!(
            plan.incomplete_path(),
            day.join("coding-session-2026-09-26-09-32-14-incomplete.mov")
        );
        assert_eq!(
            plan.fallback_path(),
            day.join("coding-session-2026-09-26-09-32-14.mov")
        );
    }

    #[test]
    fn avoids_overwriting_existing_recordings() {
        let taken = PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.mp4");
        let plan = plan_output(Path::new("/rec"), &time(), "mp4", |p| p == taken);
        assert_eq!(
            plan.final_path,
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14-2.mp4")
        );
    }

    #[test]
    fn plans_raw_screen_and_camera_files_and_the_export_target() {
        let files = plan_recording(Path::new("/rec"), &time(), "mov", true, |_| false);
        let day = PathBuf::from("/rec/2026-09-26");
        let raw = day.join("raw");
        assert_eq!(
            files.screen.final_path,
            raw.join("coding-session-2026-09-26-09-32-14-screen.mp4")
        );
        assert_eq!(
            files.screen.partial_path,
            raw.join("coding-session-2026-09-26-09-32-14-screen.partial.mov")
        );
        assert_eq!(
            files.camera.unwrap().final_path,
            raw.join("coding-session-2026-09-26-09-32-14-camera.mp4")
        );
        assert_eq!(
            files.export_path,
            day.join("coding-session-2026-09-26-09-32-14.mp4")
        );
        let without_camera = plan_recording(Path::new("/rec"), &time(), "mov", false, |_| false);
        assert!(without_camera.camera.is_none());
    }

    #[test]
    fn recording_names_avoid_existing_exports_and_raw_files() {
        let taken = PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.mp4");
        let files = plan_recording(Path::new("/rec"), &time(), "mov", false, |p| p == taken);
        assert!(files
            .export_path
            .to_string_lossy()
            .ends_with("coding-session-2026-09-26-09-32-14-2.mp4"));
        assert!(files
            .screen
            .final_path
            .to_string_lossy()
            .ends_with("-14-2-screen.mp4"));
    }

    #[test]
    fn unique_paths_add_a_counter() {
        let path = Path::new("/rec/video.mp4");
        assert_eq!(unique_path(path, |_| false), path);
        assert_eq!(
            unique_path(path, |p| p == path),
            PathBuf::from("/rec/video-2.mp4")
        );
    }

    #[test]
    fn prepare_creates_folder_and_rejects_low_disk_space() {
        let base = temp_dir("prepare");
        let plan = plan_output(&base, &time(), "mp4", |p| p.exists());
        prepare_output(&plan, Some(u64::MAX)).unwrap();
        assert!(plan.final_path.parent().unwrap().is_dir());

        let error = prepare_output(&plan, Some(10 * 1024 * 1024)).unwrap_err();
        assert!(matches!(error, AppError::Storage(_)));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn prepare_rejects_unwritable_locations() {
        let base = temp_dir("unwritable");
        let file = base.join("not-a-dir");
        fs::write(&file, b"x").unwrap();
        let plan = plan_output(&file, &time(), "mp4", |_| false);
        assert!(matches!(
            prepare_output(&plan, None),
            Err(AppError::Storage(_))
        ));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn keeps_fallback_and_incomplete_files() {
        let base = temp_dir("keep");
        let plan = plan_output(&base, &time(), "mov", |p| p.exists());
        prepare_output(&plan, None).unwrap();

        fs::write(&plan.partial_path, b"media").unwrap();
        let kept = keep_fallback(&plan).unwrap();
        assert!(kept.to_string_lossy().ends_with("-14.mov"));
        assert!(!plan.partial_path.exists());

        // The next plan must not reuse a name taken by the fallback file.
        let plan = plan_output(&base, &time(), "mov", |p| p.exists());
        assert!(plan.final_path.to_string_lossy().ends_with("-14-2.mp4"));
        fs::write(&plan.partial_path, b"media").unwrap();
        let kept = keep_incomplete(&plan).unwrap();
        assert!(kept.to_string_lossy().ends_with("-incomplete.mov"));

        let plan = plan_output(&base, &time(), "mov", |p| p.exists());
        fs::write(&plan.partial_path, b"").unwrap();
        assert_eq!(keep_incomplete(&plan), None);
        assert!(!plan.partial_path.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn recovers_partial_recordings_after_a_crash() {
        let base = temp_dir("recover");
        let day = base.join("2026-09-25");
        fs::create_dir_all(&day).unwrap();
        fs::write(day.join("coding-session-a.partial.mov"), b"media").unwrap();
        fs::write(day.join("coding-session-b.partial.mov"), b"").unwrap();
        fs::write(day.join("coding-session-c.mp4"), b"done").unwrap();
        fs::write(day.join("notes.partial.txt"), b"not ours").unwrap();

        let raw = day.join("raw");
        fs::create_dir_all(&raw).unwrap();
        fs::write(raw.join("coding-session-d-camera.partial.mov"), b"media").unwrap();

        let recovered = recover_partial_recordings(&base, |from, to| {
            fs::copy(from, to)?;
            Ok(())
        });
        assert_eq!(
            recovered,
            vec![
                day.join("coding-session-a-recovered.mp4"),
                raw.join("coding-session-d-camera-recovered.mp4"),
            ]
        );
        assert!(!day.join("coding-session-a.partial.mov").exists());
        assert!(!day.join("coding-session-b.partial.mov").exists());
        assert!(day.join("coding-session-c.mp4").exists());
        assert!(day.join("notes.partial.txt").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn keeps_the_original_file_when_recovery_conversion_fails() {
        let base = temp_dir("recover-fallback");
        let day = base.join("2026-09-25");
        fs::create_dir_all(&day).unwrap();
        fs::write(day.join("coding-session-a.partial.mov"), b"media").unwrap();

        let recovered =
            recover_partial_recordings(&base, |_, _| Err(AppError::Encoder("unreadable".into())));
        assert_eq!(recovered, vec![day.join("coding-session-a-recovered.mov")]);
        assert!(recovered[0].exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn recovery_ignores_missing_directories() {
        assert!(
            recover_partial_recordings(Path::new("/definitely/not/here"), |_, _| Ok(())).is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_available_space_for_nonexistent_subfolders() {
        let space = available_space(&std::env::temp_dir().join("does/not/exist/yet"));
        assert!(space.unwrap_or(0) > 0);
    }
}
