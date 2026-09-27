//! Renders the final video: the webcam composited onto the screen recording,
//! at the position, size and shape chosen in the review step, with the
//! audio re-mixed (voice volume, optional music).
//!
//! Both raw files share one timeline (the recorder writes them with the
//! same timestamps), so for every screen frame the latest camera frame at
//! or before it is blended on top. Frames are decoded to BGRA, composited
//! on the CPU with the same code as the rest of CodeTake, and encoded with
//! the H.264 encoder in export mode (a plain MP4, nothing dropped).

use std::fs;

use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_av_foundation::{
    AVAsset, AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVMediaTypeAudio,
    AVMediaTypeVideo,
};
use objc2_core_foundation::CFString;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA};
use objc2_foundation::NSString;

use super::encoder::{dict, int, Dict, MacEncoder};
use super::export::{fail, file_url, first_track, pcm_output_settings, seconds, temporary_path};
use super::frame::PixelBufferFrame;
use super::util::{audio_samples, describe_error};
use crate::audio::convert::Converter;
use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::config::{video_bitrate, Size};
use crate::error::AppResult;
use crate::export::{mix_block, music_block, CompositeJob, MusicBed};
use crate::recording::capture::{EncoderSettings, VideoEncoder, VideoFrame};
use crate::video::compositor::{self, Mask};

/// Frames of music generated per block when the recording has no audio.
const MUSIC_BLOCK_FRAMES: usize = 4096;

fn bgra_output_settings() -> Retained<Dict> {
    // SAFETY: CoreVideo keys are toll-free bridged CFStrings.
    let key: &NSString =
        unsafe { &*(kCVPixelBufferPixelFormatTypeKey as *const CFString).cast::<NSString>() };
    dict(vec![(key, int(i64::from(kCVPixelFormatType_32BGRA)))])
}

fn reader_output(
    reader: &AVAssetReader,
    track: &objc2_av_foundation::AVAssetTrack,
    settings: &Dict,
) -> Retained<AVAssetReaderTrackOutput> {
    // SAFETY: valid reader, track and settings.
    unsafe {
        let output = AVAssetReaderTrackOutput::initWithTrack_outputSettings(
            AVAssetReaderTrackOutput::alloc(),
            track,
            Some(settings),
        );
        output.setAlwaysCopiesSampleData(false);
        reader.addOutput(&output);
        output
    }
}

fn start(reader: &AVAssetReader, what: &str) -> AppResult<()> {
    // SAFETY: plain calls on a configured reader.
    unsafe {
        if reader.startReading() {
            Ok(())
        } else {
            Err(fail(format!(
                "cannot read the {what}: {}",
                reader
                    .error()
                    .map(|e| describe_error(&e))
                    .unwrap_or_default()
            )))
        }
    }
}

/// The next sample that carries a decoded image.
fn next_frame(output: &AVAssetReaderTrackOutput) -> Option<(f64, PixelBufferFrame)> {
    loop {
        // SAFETY: plain call on a reading output.
        let sample: Retained<CMSampleBuffer> = unsafe { output.copyNextSampleBuffer() }?;
        let time = seconds(&sample);
        // SAFETY: the sample buffer is valid.
        if let Some(frame) = unsafe { sample.image_buffer() }.and_then(PixelBufferFrame::new) {
            return Some((time, frame));
        }
    }
}

fn reader_failed(reader: &AVAssetReader) -> bool {
    // SAFETY: plain property read.
    unsafe { reader.status() == AVAssetReaderStatus::Failed }
}

pub fn composite(job: &CompositeJob, progress: &mut dyn FnMut(f64)) -> AppResult<()> {
    let temporary = temporary_path(&job.destination);
    let _ = fs::remove_file(&temporary);
    match render(job, &temporary, progress) {
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

fn render(
    job: &CompositeJob,
    temporary: &std::path::Path,
    progress: &mut dyn FnMut(f64),
) -> AppResult<()> {
    // SAFETY: framework constants.
    let (video_media, audio_media) = match unsafe { (AVMediaTypeVideo, AVMediaTypeAudio) } {
        (Some(v), Some(a)) => (v, a),
        _ => return Err(fail("AVFoundation is unavailable")),
    };

    // SAFETY: valid file URLs.
    let screen_asset = unsafe { AVAsset::assetWithURL(&file_url(&job.screen)) };
    let camera_asset = unsafe { AVAsset::assetWithURL(&file_url(&job.camera)) };
    // SAFETY: plain property read.
    let duration = unsafe { screen_asset.duration().seconds() };
    if !duration.is_finite() || duration <= 0.0 {
        return Err(fail("the screen recording is empty or unreadable"));
    }
    let screen_track = first_track(&screen_asset, video_media)?
        .ok_or_else(|| fail("the screen recording has no video"))?;
    let audio_track = first_track(&screen_asset, audio_media)?;
    let camera_track = first_track(&camera_asset, video_media)?
        .ok_or_else(|| fail("the camera recording has no video"))?;

    // SAFETY: valid assets.
    let screen_reader =
        unsafe { AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &screen_asset) }
            .map_err(|e| fail(describe_error(&e)))?;
    let camera_reader =
        unsafe { AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &camera_asset) }
            .map_err(|e| fail(describe_error(&e)))?;
    let bgra = bgra_output_settings();
    let screen_output = reader_output(&screen_reader, &screen_track, &bgra);
    let audio_output = match &audio_track {
        Some(track) => Some(reader_output(
            &screen_reader,
            track,
            &*pcm_output_settings()?,
        )),
        None => None,
    };
    let camera_output = reader_output(&camera_reader, &camera_track, &bgra);
    start(&screen_reader, "screen recording")?;
    start(&camera_reader, "camera recording")?;

    let mut screen_frame =
        next_frame(&screen_output).ok_or_else(|| fail("the screen recording has no frames"))?;
    let size = Size::new(
        screen_frame.1.width() as u32,
        screen_frame.1.height() as u32,
    );
    let music = MusicBed::for_export(job.music.as_ref(), duration);
    let mut encoder = MacEncoder::new(&EncoderSettings {
        path: temporary.to_path_buf(),
        size,
        fps: job.fps,
        video_bitrate: video_bitrate(size, job.fps),
        audio: audio_track.is_some() || job.music.is_some(),
        live: false,
    })?;

    let rect = compositor::overlay_rect(size.width as usize, size.height as usize, &job.overlay);
    let mask = Mask::new(rect.width, rect.height, job.overlay.shape);

    let mut camera_current: Option<PixelBufferFrame> = None;
    let mut camera_next = next_frame(&camera_output);
    let mut converter: Option<Converter> = None;
    let mut audio_time = 0.0_f64;
    let mut audio_done = audio_output.is_none();
    let mut music_frame = 0_u64;
    let total_frames = (duration * f64::from(SAMPLE_RATE)).round() as u64;

    // Appends recorded (and music) audio up to `until` seconds.
    let mut feed_audio = |encoder: &mut MacEncoder, until: f64| -> AppResult<()> {
        if let Some(output) = &audio_output {
            while !audio_done && audio_time < until {
                // SAFETY: plain call on a reading output.
                let Some(sample) = (unsafe { output.copyNextSampleBuffer() }) else {
                    audio_done = true;
                    break;
                };
                let start = seconds(&sample);
                let Some((format, samples)) = audio_samples(&sample) else {
                    continue;
                };
                let converter = match converter.as_mut() {
                    Some(c) if c.input_format() == format => c,
                    _ => converter.insert(Converter::new(format)),
                };
                let recorded = converter.convert(&samples);
                let frame = (start * f64::from(SAMPLE_RATE)).round().max(0.0) as u64;
                let mixed = mix_block(&recorded, job.recording_volume, &music, frame);
                encoder.append_audio(start, &mixed)?;
                audio_time = start + (mixed.len() / CHANNELS) as f64 / f64::from(SAMPLE_RATE);
            }
        } else if job.music.is_some() {
            let limit = ((until.min(duration)) * f64::from(SAMPLE_RATE)) as u64;
            while music_frame < limit.min(total_frames) {
                let frames = MUSIC_BLOCK_FRAMES.min((total_frames - music_frame) as usize);
                let start = music_frame as f64 / f64::from(SAMPLE_RATE);
                encoder.append_audio(start, &music_block(frames, &music, music_frame))?;
                music_frame += frames as u64;
            }
        }
        Ok(())
    };

    loop {
        let (time, frame) = &screen_frame;
        feed_audio(&mut encoder, time + 0.05)?;

        // The latest camera frame at or before this screen frame.
        while let Some((camera_time, _)) = &camera_next {
            if *camera_time > time + 1e-3 {
                break;
            }
            camera_current = camera_next.take().map(|(_, f)| f);
            camera_next = next_frame(&camera_output);
        }

        let camera = camera_current.as_ref();
        let mut draw = |dst: &mut compositor::BgraMut<'_>| {
            frame.read(&mut |src| compositor::copy_frame(src, dst));
            if let Some(camera) = camera {
                camera.read(&mut |src| compositor::blend_overlay(dst, src, rect, &mask, true));
            }
        };
        encoder.append_video(*time, &mut draw)?;
        progress((time / duration).clamp(0.0, 0.99));

        match next_frame(&screen_output) {
            Some(next) => screen_frame = next,
            None => break,
        }
    }
    feed_audio(&mut encoder, f64::INFINITY)?;

    if reader_failed(&screen_reader) || reader_failed(&camera_reader) {
        return Err(fail("reading the raw recordings failed"));
    }
    encoder.finish(duration)?;
    progress(1.0);
    Ok(())
}
