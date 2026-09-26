//! Writes a copy of a recording with background music mixed in.
//!
//! The H.264 video samples are copied as they are (no re-encoding, so this is
//! fast and lossless); the recorded audio is decoded, mixed with the music
//! and encoded to AAC again. Reading and writing are interleaved by
//! timestamp so `AVAssetWriter` never waits for one track while the other
//! runs ahead.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_av_foundation::{
    AVAsset, AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack,
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaType,
    AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_avf_audio::{
    AVFormatIDKey, AVLinearPCMBitDepthKey, AVLinearPCMIsBigEndianKey, AVLinearPCMIsFloatKey,
    AVLinearPCMIsNonInterleaved, AVNumberOfChannelsKey, AVSampleRateKey,
};
use objc2_core_audio_types::kAudioFormatLinearPCM;
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{NSArray, NSError, NSNumber, NSString, NSURL};

use super::encoder::{audio_settings, dict, int, key, object, pcm_format, pcm_sample_buffer, Dict};
use super::util::{audio_samples, cm_time, describe_error, wait_for};
use crate::audio::convert::Converter;
use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::error::{AppError, AppResult};
use crate::export::{mix_block, music_block, ExportJob, MusicBed};

/// Frames of music generated per block when the recording has no audio.
const MUSIC_BLOCK_FRAMES: usize = 4096;

pub(super) fn fail(message: impl Into<String>) -> AppError {
    AppError::Encoder(format!("export failed: {}", message.into()))
}

pub(super) fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// A track handed from AVFoundation's completion queue to our thread.
struct SentTrack(Option<Retained<AVAssetTrack>>);

// SAFETY: the track is only moved to the waiting thread once and not used
// concurrently; AVAssetTrack is an immutable description of the asset.
unsafe impl Send for SentTrack {}

pub(super) fn first_track(
    asset: &AVAsset,
    media: &AVMediaType,
) -> AppResult<Option<Retained<AVAssetTrack>>> {
    let result = wait_for(Duration::from_secs(30), |tx| {
        let handler = RcBlock::new(
            move |tracks: *mut NSArray<AVAssetTrack>, error: *mut NSError| {
                // SAFETY: AVFoundation passes a valid array or a valid error.
                let result = unsafe {
                    match tracks.as_ref() {
                        Some(tracks) => Ok(SentTrack(tracks.firstObject())),
                        None => Err(error.as_ref().map(describe_error).unwrap_or_default()),
                    }
                };
                let _ = tx.send(result);
            },
        );
        // SAFETY: valid media type; the block is 'static and called once.
        unsafe { asset.loadTracksWithMediaType_completionHandler(media, &handler) };
    });
    match result {
        Some(Ok(track)) => Ok(track.0),
        Some(Err(message)) => Err(fail(format!("cannot read the recording: {message}"))),
        None => Err(fail("timed out reading the recording")),
    }
}

pub(super) fn pcm_output_settings() -> AppResult<Retained<Dict>> {
    let number = |v: f64| object(NSNumber::new_f64(v));
    let boolean = |v: bool| object(NSNumber::new_bool(v));
    // SAFETY: reading framework constant strings.
    unsafe {
        Ok(dict(vec![
            (key(AVFormatIDKey)?, int(i64::from(kAudioFormatLinearPCM))),
            (key(AVSampleRateKey)?, number(f64::from(SAMPLE_RATE))),
            (key(AVNumberOfChannelsKey)?, int(CHANNELS as i64)),
            (key(AVLinearPCMBitDepthKey)?, int(32)),
            (key(AVLinearPCMIsFloatKey)?, boolean(true)),
            (key(AVLinearPCMIsNonInterleaved)?, boolean(false)),
            (key(AVLinearPCMIsBigEndianKey)?, boolean(false)),
        ]))
    }
}

pub(super) fn seconds(sample: &CMSampleBuffer) -> f64 {
    // SAFETY: the sample buffer is valid.
    unsafe { sample.presentation_time_stamp().seconds() }
}

/// Where the export is written until it is complete. Hidden, and not named
/// like a partial recording, so crash recovery never picks it up.
pub(super) fn temporary_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    destination.with_file_name(format!(".{name}"))
}

pub fn export(job: &ExportJob, progress: &mut dyn FnMut(f64)) -> AppResult<()> {
    let temporary = temporary_path(&job.destination);
    let _ = fs::remove_file(&temporary);
    let result = write(job, &temporary, progress);
    match result {
        Ok(()) => {
            fs::rename(&temporary, &job.destination)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn write(job: &ExportJob, temporary: &Path, progress: &mut dyn FnMut(f64)) -> AppResult<()> {
    // SAFETY: all Objective-C calls below use valid objects created here;
    // sample buffers are used only while retained.
    unsafe {
        let (video_media, audio_media) = match (AVMediaTypeVideo, AVMediaTypeAudio) {
            (Some(v), Some(a)) => (v, a),
            _ => return Err(fail("AVFoundation is unavailable")),
        };
        let asset = AVAsset::assetWithURL(&file_url(&job.source));
        let duration = asset.duration().seconds();
        if !duration.is_finite() || duration <= 0.0 {
            return Err(fail("the recording is empty or unreadable"));
        }
        let video_track =
            first_track(&asset, video_media)?.ok_or_else(|| fail("the recording has no video"))?;
        let audio_track = first_track(&asset, audio_media)?;

        let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset)
            .map_err(|e| fail(describe_error(&e)))?;
        let video_output = AVAssetReaderTrackOutput::initWithTrack_outputSettings(
            AVAssetReaderTrackOutput::alloc(),
            &video_track,
            None,
        );
        video_output.setAlwaysCopiesSampleData(false);
        reader.addOutput(&video_output);
        let audio_output = match &audio_track {
            Some(track) => {
                let output = AVAssetReaderTrackOutput::initWithTrack_outputSettings(
                    AVAssetReaderTrackOutput::alloc(),
                    track,
                    Some(&*pcm_output_settings()?),
                );
                reader.addOutput(&output);
                Some(output)
            }
            None => None,
        };
        if !reader.startReading() {
            return Err(fail(
                reader
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "cannot read the recording".into()),
            ));
        }

        // Passthrough readers may start with empty marker buffers; the
        // writer needs the format of the first real frame.
        let mut pending_video = video_output.copyNextSampleBuffer();
        while pending_video
            .as_ref()
            .is_some_and(|s| s.format_description().is_none() && s.num_samples() == 0)
        {
            pending_video = video_output.copyNextSampleBuffer();
        }
        let first_video = pending_video
            .as_ref()
            .ok_or_else(|| fail("the recording has no video frames"))?;

        let file_type = AVFileTypeMPEG4.ok_or_else(|| fail("MP4 is unavailable"))?;
        let writer = AVAssetWriter::initWithURL_fileType_error(
            AVAssetWriter::alloc(),
            &file_url(temporary),
            file_type,
        )
        .map_err(|e| fail(describe_error(&e)))?;
        writer.setShouldOptimizeForNetworkUse(true);
        let video_input = AVAssetWriterInput::initWithMediaType_outputSettings_sourceFormatHint(
            AVAssetWriterInput::alloc(),
            video_media,
            None,
            first_video.format_description().as_deref(),
        );
        video_input.setTransform(video_track.preferredTransform());
        let audio_input = AVAssetWriterInput::initWithMediaType_outputSettings(
            AVAssetWriterInput::alloc(),
            audio_media,
            Some(&*audio_settings()?),
        );
        if !writer.canAddInput(&video_input) {
            return Err(fail(format!(
                "cannot copy this video track into an MP4 (hint present: {})",
                first_video.format_description().is_some()
            )));
        }
        if !writer.canAddInput(&audio_input) {
            return Err(fail("cannot add an AAC audio track"));
        }
        writer.addInput(&video_input);
        writer.addInput(&audio_input);
        if !writer.startWriting() {
            return Err(fail(
                writer
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "cannot create the file".into()),
            ));
        }
        writer.startSessionAtSourceTime(CMTime::new(0, 1));

        let music = MusicBed::for_export(job.music.as_ref(), duration);
        let pcm = pcm_format()?;
        let total_frames = (duration * f64::from(SAMPLE_RATE)).round() as u64;
        let mut converter: Option<Converter> = None;
        let mut audio_time = 0.0_f64; // end of the audio written so far
        let mut music_frame = 0_u64; // next frame when generating music only
        let mut video_done = false;
        let mut audio_done = false;

        while !(video_done && audio_done) {
            if writer.status() == AVAssetWriterStatus::Failed {
                break;
            }
            let video_time = pending_video.as_deref().map_or(f64::INFINITY, seconds);
            let prefer_video = !video_done && (audio_done || video_time <= audio_time);
            let video_ready = !video_done && video_input.isReadyForMoreMediaData();
            let audio_ready = !audio_done && audio_input.isReadyForMoreMediaData();

            if video_ready && (prefer_video || !audio_ready) {
                match pending_video.take() {
                    Some(sample) => {
                        if !video_input.appendSampleBuffer(&sample) {
                            break;
                        }
                        progress((seconds(&sample) / duration).clamp(0.0, 1.0));
                        pending_video = video_output.copyNextSampleBuffer();
                    }
                    None => {
                        video_input.markAsFinished();
                        video_done = true;
                    }
                }
            } else if audio_ready {
                let block = match &audio_output {
                    Some(output) => match output.copyNextSampleBuffer() {
                        Some(sample) => {
                            let start = seconds(&sample);
                            audio_samples(&sample).map(|(format, samples)| {
                                let converter = match converter.as_mut() {
                                    Some(c) if c.input_format() == format => c,
                                    _ => converter.insert(Converter::new(format)),
                                };
                                let recorded = converter.convert(&samples);
                                let frame =
                                    (start * f64::from(SAMPLE_RATE)).round().max(0.0) as u64;
                                (
                                    start,
                                    mix_block(&recorded, job.recording_volume, &music, frame),
                                )
                            })
                        }
                        None => {
                            audio_done = true;
                            None
                        }
                    },
                    None if music_frame < total_frames => {
                        let frames = MUSIC_BLOCK_FRAMES.min((total_frames - music_frame) as usize);
                        let start = music_frame as f64 / f64::from(SAMPLE_RATE);
                        let block = music_block(frames, &music, music_frame);
                        music_frame += frames as u64;
                        Some((start, block))
                    }
                    None => {
                        audio_done = true;
                        None
                    }
                };
                if let Some((start, samples)) = block {
                    if !samples.is_empty() {
                        let buffer = pcm_sample_buffer(&pcm, start, &samples)?;
                        if !audio_input.appendSampleBuffer(&buffer) {
                            break;
                        }
                        audio_time =
                            start + (samples.len() / CHANNELS) as f64 / f64::from(SAMPLE_RATE);
                    }
                }
                if audio_done {
                    audio_input.markAsFinished();
                }
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }

        if reader.status() == AVAssetReaderStatus::Failed {
            writer.cancelWriting();
            return Err(fail(
                reader
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "reading the recording failed".into()),
            ));
        }
        if writer.status() == AVAssetWriterStatus::Failed {
            return Err(fail(
                writer
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "writing failed".into()),
            ));
        }

        writer.endSessionAtSourceTime(cm_time(duration));
        let finishing = writer.clone();
        let done = wait_for(Duration::from_secs(120), move |tx| {
            let handler = RcBlock::new(move || {
                let _ = tx.send(());
            });
            finishing.finishWritingWithCompletionHandler(&handler);
        });
        if done.is_none() || writer.status() != AVAssetWriterStatus::Completed {
            return Err(fail(
                writer
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_else(|| "could not finish the file".into()),
            ));
        }
        progress(1.0);
        Ok(())
    }
}
