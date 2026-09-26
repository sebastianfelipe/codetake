//! Converts the recorded QuickTime movie into an MP4 without re-encoding.

use std::path::Path;
use std::time::Duration;

use block2::RcBlock;
use objc2_av_foundation::{
    AVAsset, AVAssetExportPresetPassthrough, AVAssetExportSession, AVAssetExportSessionStatus,
    AVFileTypeMPEG4,
};
use objc2_foundation::{NSString, NSURL};

use super::util::{describe_error, wait_for};
use crate::error::{AppError, AppResult};

fn file_url(path: &Path) -> objc2::rc::Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// Copies the H.264 and AAC samples of `source` into a new MP4 at
/// `destination` ("passthrough" export: fast and lossless).
pub fn to_mp4(source: &Path, destination: &Path) -> AppResult<()> {
    let fail = |message: String| AppError::Encoder(format!("MP4 conversion failed: {message}"));
    // SAFETY: valid URLs, preset and file type constants; the session is kept
    // alive until the completion handler has run.
    unsafe {
        let asset = AVAsset::assetWithURL(&file_url(source));
        let session = AVAssetExportSession::exportSessionWithAsset_presetName(
            &asset,
            AVAssetExportPresetPassthrough,
        )
        .ok_or_else(|| fail("the recording cannot be exported".into()))?;
        let file_type = AVFileTypeMPEG4.ok_or_else(|| fail("MP4 is unavailable".into()))?;
        session.setOutputURL(Some(&file_url(destination)));
        session.setOutputFileType(Some(file_type));
        // Put the index at the start of the file so players and upload sites
        // can start without reading the whole file.
        session.setShouldOptimizeForNetworkUse(true);

        let exporter = session.clone();
        // Long recordings take a while to copy; allow plenty of time.
        let done = wait_for(Duration::from_secs(60 * 60), move |tx| {
            let handler = RcBlock::new(move || {
                let _ = tx.send(());
            });
            #[allow(deprecated)]
            exporter.exportAsynchronouslyWithCompletionHandler(&handler);
        });
        if done.is_none() {
            session.cancelExport();
            return Err(fail("timed out".into()));
        }
        match session.status() {
            AVAssetExportSessionStatus::Completed => Ok(()),
            _ => Err(fail(
                session
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "unknown error".into()),
            )),
        }
    }
}
