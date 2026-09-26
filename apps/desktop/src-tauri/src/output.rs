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
//! While a recording is in progress it is written to a `.partial.mp4` file
//! next to its final location and renamed once the file has been finalized.
//! If CodeTake exits unexpectedly, the partial file is left behind and is
//! recovered (renamed) on the next launch instead of being deleted.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::error::{AppError, AppResult};

const FILE_PREFIX: &str = "coding-session";
const PARTIAL_SUFFIX: &str = ".partial.mp4";
const RECOVERED_SUFFIX: &str = "-recovered.mp4";
const INCOMPLETE_SUFFIX: &str = "-incomplete.mp4";

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
    /// Path used when a recording could not be finalized cleanly but media
    /// was written; the file is kept rather than deleted.
    pub fn incomplete_path(&self) -> PathBuf {
        with_suffix(&self.final_path, INCOMPLETE_SUFFIX)
    }
}

fn with_suffix(final_path: &Path, suffix: &str) -> PathBuf {
    let stem = final_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    final_path.with_file_name(format!("{stem}{suffix}"))
}

/// Builds the output paths for a recording started at `time`.
///
/// Does not touch the file system; if the name is already taken (two
/// recordings in the same second), `exists` lets the caller add a counter.
pub fn plan_output(
    base: &Path,
    time: &DateTime<Local>,
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
        let final_path = folder.join(format!("{name}.mp4"));
        let partial_path = folder.join(format!("{name}{PARTIAL_SUFFIX}"));
        if !exists(&final_path) && !exists(&partial_path) {
            return OutputPlan {
                final_path,
                partial_path,
            };
        }
        counter += 1;
    }
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

/// Moves the finalized partial file to its final name.
pub fn finalize(plan: &OutputPlan) -> AppResult<PathBuf> {
    fs::rename(&plan.partial_path, &plan.final_path)?;
    Ok(plan.final_path.clone())
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

/// Removes the partial file of a recording that never produced media.
pub fn discard(plan: &OutputPlan) {
    let _ = fs::remove_file(&plan.partial_path);
}

/// Finds partial recordings left behind by a crash and renames them so they
/// are visible to the user. Empty partial files are deleted.
pub fn recover_partial_recordings(base: &Path) -> Vec<PathBuf> {
    let mut recovered = Vec::new();
    let Ok(days) = fs::read_dir(base) else {
        return recovered;
    };
    for day in days.flatten() {
        let Ok(entries) = fs::read_dir(day.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(PARTIAL_SUFFIX) else {
                continue;
            };
            if entry.metadata().map(|m| m.len() == 0).unwrap_or(true) {
                let _ = fs::remove_file(&path);
                continue;
            }
            let target = path.with_file_name(format!("{stem}{RECOVERED_SUFFIX}"));
            if fs::rename(&path, &target).is_ok() {
                recovered.push(target);
            }
        }
    }
    recovered.sort();
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
        let plan = plan_output(Path::new("/rec"), &time(), |_| false);
        assert_eq!(
            plan.final_path,
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.mp4")
        );
        assert_eq!(
            plan.partial_path,
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.partial.mp4")
        );
        assert_eq!(
            plan.incomplete_path(),
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14-incomplete.mp4")
        );
    }

    #[test]
    fn avoids_overwriting_existing_recordings() {
        let taken = PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14.mp4");
        let plan = plan_output(Path::new("/rec"), &time(), |p| p == taken);
        assert_eq!(
            plan.final_path,
            PathBuf::from("/rec/2026-09-26/coding-session-2026-09-26-09-32-14-2.mp4")
        );
    }

    #[test]
    fn prepare_creates_folder_and_rejects_low_disk_space() {
        let base = temp_dir("prepare");
        let plan = plan_output(&base, &time(), |p| p.exists());
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
        let plan = plan_output(&file, &time(), |_| false);
        assert!(matches!(
            prepare_output(&plan, None),
            Err(AppError::Storage(_))
        ));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn finalize_and_keep_incomplete_move_the_partial_file() {
        let base = temp_dir("finalize");
        let plan = plan_output(&base, &time(), |p| p.exists());
        prepare_output(&plan, None).unwrap();

        fs::write(&plan.partial_path, b"media").unwrap();
        assert_eq!(finalize(&plan).unwrap(), plan.final_path);
        assert!(plan.final_path.exists() && !plan.partial_path.exists());

        let plan = plan_output(&base, &time(), |p| p.exists());
        fs::write(&plan.partial_path, b"media").unwrap();
        let kept = keep_incomplete(&plan).unwrap();
        assert!(kept.to_string_lossy().ends_with("-incomplete.mp4"));

        let plan = plan_output(&base, &time(), |p| p.exists());
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
        fs::write(day.join("coding-session-a.partial.mp4"), b"media").unwrap();
        fs::write(day.join("coding-session-b.partial.mp4"), b"").unwrap();
        fs::write(day.join("coding-session-c.mp4"), b"done").unwrap();

        let recovered = recover_partial_recordings(&base);
        assert_eq!(recovered, vec![day.join("coding-session-a-recovered.mp4")]);
        assert!(!day.join("coding-session-b.partial.mp4").exists());
        assert!(day.join("coding-session-c.mp4").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn recovery_ignores_missing_directories() {
        assert!(recover_partial_recordings(Path::new("/definitely/not/here")).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn reports_available_space_for_nonexistent_subfolders() {
        let space = available_space(&std::env::temp_dir().join("does/not/exist/yet"));
        assert!(space.unwrap_or(0) > 0);
    }
}
