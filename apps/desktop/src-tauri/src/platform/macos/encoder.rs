//! H.264/AAC encoding with `AVAssetWriter` (hardware accelerated through
//! VideoToolbox).
//!
//! Media is written progressively as a fragmented QuickTime movie (a new
//! fragment every [`FRAGMENT_SECONDS`]), so if CodeTake is killed
//! mid-recording everything up to the last fragment is still playable. When
//! recording stops the movie is losslessly remuxed into the final MP4 (see
//! `remux.rs`).
//!
//! Why not write fragmented MP4 directly? `AVAssetWriter` intermittently
//! fails to finalize fragmented MPEG-4 files (`-11800` with underlying
//! `-16341`) once at least one fragment has been flushed; in our tests
//! about a third of 4-second recordings failed. Fragmented QuickTime movies
//! with identical settings finalized reliably.

use std::ptr::{self, NonNull};
use std::thread;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeQuickTimeMovie, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAverageBitRateKey,
    AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoColorPrimariesKey,
    AVVideoColorPrimaries_ITU_R_709_2, AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey,
    AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey,
    AVVideoProfileLevelH264HighAutoLevel, AVVideoProfileLevelKey, AVVideoTransferFunctionKey,
    AVVideoTransferFunction_ITU_R_709_2, AVVideoWidthKey, AVVideoYCbCrMatrixKey,
    AVVideoYCbCrMatrix_ITU_R_709_2,
};
use objc2_avf_audio::{AVEncoderBitRateKey, AVFormatIDKey, AVNumberOfChannelsKey, AVSampleRateKey};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
    AudioStreamBasicDescription,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescriptionCreate, CMAudioSampleBufferCreateReadyWithPacketDescriptions,
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime,
};
use objc2_core_video::{
    kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
    CVPixelBuffer, CVPixelBufferPool,
};
use objc2_foundation::{NSCopying, NSDictionary, NSNumber, NSString, NSURL};

use super::frame::write_pixel_buffer;
use super::util::{cm_time, describe_error, wait_for};
use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::error::{AppError, AppResult};
use crate::recording::capture::{EncoderSettings, VideoEncoder};
use crate::video::compositor::BgraMut;

const FRAGMENT_SECONDS: i64 = 2;
const AUDIO_BITRATE: i64 = 192_000;

type Dict = NSDictionary<NSString, AnyObject>;

fn object<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(value) }
}

fn int(value: i64) -> Retained<AnyObject> {
    object(NSNumber::new_i64(value))
}

fn dict(entries: Vec<(&NSString, Retained<AnyObject>)>) -> Retained<Dict> {
    let keys: Vec<&NSString> = entries.iter().map(|(k, _)| *k).collect();
    let values: Vec<Retained<AnyObject>> = entries.into_iter().map(|(_, v)| v).collect();
    NSDictionary::from_retained_objects(&keys, &values)
}

fn key(value: Option<&'static NSString>) -> AppResult<&'static NSString> {
    value.ok_or_else(|| AppError::Encoder("AVFoundation constant unavailable".into()))
}

fn video_settings(settings: &EncoderSettings) -> AppResult<Retained<Dict>> {
    let fps = i64::from(settings.fps.as_u32());
    // SAFETY: reading framework constant strings.
    unsafe {
        let compression = dict(vec![
            (
                key(AVVideoAverageBitRateKey)?,
                int(i64::from(settings.video_bitrate)),
            ),
            (key(AVVideoExpectedSourceFrameRateKey)?, int(fps)),
            // A keyframe every two seconds keeps seeking responsive.
            (key(AVVideoMaxKeyFrameIntervalKey)?, int(fps * 2)),
            (
                key(AVVideoProfileLevelKey)?,
                object(key(AVVideoProfileLevelH264HighAutoLevel)?.copy()),
            ),
        ]);
        let color = dict(vec![
            (
                key(AVVideoColorPrimariesKey)?,
                object(key(AVVideoColorPrimaries_ITU_R_709_2)?.copy()),
            ),
            (
                key(AVVideoTransferFunctionKey)?,
                object(key(AVVideoTransferFunction_ITU_R_709_2)?.copy()),
            ),
            (
                key(AVVideoYCbCrMatrixKey)?,
                object(key(AVVideoYCbCrMatrix_ITU_R_709_2)?.copy()),
            ),
        ]);
        Ok(dict(vec![
            (
                key(AVVideoCodecKey)?,
                object(key(AVVideoCodecTypeH264)?.copy()),
            ),
            (key(AVVideoWidthKey)?, int(i64::from(settings.size.width))),
            (key(AVVideoHeightKey)?, int(i64::from(settings.size.height))),
            (key(AVVideoCompressionPropertiesKey)?, object(compression)),
            (key(AVVideoColorPropertiesKey)?, object(color)),
        ]))
    }
}

fn audio_settings() -> AppResult<Retained<Dict>> {
    // SAFETY: reading framework constant strings.
    unsafe {
        Ok(dict(vec![
            (key(AVFormatIDKey)?, int(i64::from(kAudioFormatMPEG4AAC))),
            (key(AVSampleRateKey)?, int(i64::from(SAMPLE_RATE))),
            (key(AVNumberOfChannelsKey)?, int(CHANNELS as i64)),
            (key(AVEncoderBitRateKey)?, int(AUDIO_BITRATE)),
        ]))
    }
}

fn pixel_buffer_attributes(settings: &EncoderSettings) -> Retained<Dict> {
    // SAFETY: CoreVideo keys are toll-free bridged CFStrings.
    unsafe {
        let cf_key = |k: &'static objc2_core_foundation::CFString| -> &'static NSString {
            &*(k as *const objc2_core_foundation::CFString).cast::<NSString>()
        };
        dict(vec![
            (
                cf_key(kCVPixelBufferPixelFormatTypeKey),
                int(i64::from(kCVPixelFormatType_32BGRA)),
            ),
            (
                cf_key(kCVPixelBufferWidthKey),
                int(i64::from(settings.size.width)),
            ),
            (
                cf_key(kCVPixelBufferHeightKey),
                int(i64::from(settings.size.height)),
            ),
            (
                cf_key(kCVPixelBufferIOSurfacePropertiesKey),
                object(NSDictionary::<NSString, AnyObject>::new()),
            ),
        ])
    }
}

/// Interleaved stereo float PCM at the mixer sample rate.
fn pcm_format() -> AppResult<CFRetained<CMFormatDescription>> {
    let bytes_per_frame = (CHANNELS * std::mem::size_of::<f32>()) as u32;
    let asbd = AudioStreamBasicDescription {
        mSampleRate: f64::from(SAMPLE_RATE),
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: CHANNELS as u32,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    let mut out: *const CMFormatDescription = ptr::null();
    // SAFETY: `asbd` and `out` are valid for the call; the result follows the
    // Create rule and is owned by the returned CFRetained.
    unsafe {
        let status = CMAudioFormatDescriptionCreate(
            None,
            NonNull::from(&asbd),
            0,
            ptr::null(),
            0,
            ptr::null(),
            None,
            NonNull::from(&mut out),
        );
        match NonNull::new(out.cast_mut()) {
            Some(format) if status == 0 => Ok(CFRetained::from_raw(format)),
            _ => Err(AppError::Encoder(format!(
                "could not describe the audio format (status {status})"
            ))),
        }
    }
}

pub struct MacEncoder {
    writer: Retained<AVAssetWriter>,
    video_input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    audio: Option<(
        Retained<AVAssetWriterInput>,
        CFRetained<CMFormatDescription>,
    )>,
    finished: bool,
}

// SAFETY: AVAssetWriter and its inputs may be used from any thread as long as
// calls are not concurrent; the recorder serializes access with a mutex.
unsafe impl Send for MacEncoder {}

impl MacEncoder {
    pub fn new(settings: &EncoderSettings) -> AppResult<Self> {
        let path = NSString::from_str(&settings.path.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        // SAFETY: valid file URL and file type constant.
        let writer = unsafe {
            let file_type = AVFileTypeQuickTimeMovie
                .ok_or_else(|| AppError::Encoder("QuickTime output is unavailable".into()))?;
            AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url, file_type)
                .map_err(|e| AppError::Encoder(describe_error(&e)))?
        };
        // SAFETY: plain property.
        unsafe { writer.setMovieFragmentInterval(CMTime::new(FRAGMENT_SECONDS, 1)) };

        // SAFETY: valid media type constants and settings dictionaries.
        let (video_input, adaptor) = unsafe {
            let media = AVMediaTypeVideo
                .ok_or_else(|| AppError::Encoder("video output is unavailable".into()))?;
            let input = AVAssetWriterInput::initWithMediaType_outputSettings(
                AVAssetWriterInput::alloc(),
                media,
                Some(&*video_settings(settings)?),
            );
            input.setExpectsMediaDataInRealTime(true);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &input,
                Some(&pixel_buffer_attributes(settings)),
            );
            if !writer.canAddInput(&input) {
                return Err(AppError::Encoder(format!(
                    "the encoder does not support {}×{} video",
                    settings.size.width, settings.size.height
                )));
            }
            writer.addInput(&input);
            (input, adaptor)
        };

        let audio = if settings.audio {
            // SAFETY: valid media type constant and settings dictionary.
            unsafe {
                let media = AVMediaTypeAudio
                    .ok_or_else(|| AppError::Encoder("audio output is unavailable".into()))?;
                let input = AVAssetWriterInput::initWithMediaType_outputSettings(
                    AVAssetWriterInput::alloc(),
                    media,
                    Some(&*audio_settings()?),
                );
                input.setExpectsMediaDataInRealTime(true);
                if !writer.canAddInput(&input) {
                    return Err(AppError::Encoder("cannot add an AAC audio track".into()));
                }
                writer.addInput(&input);
                Some((input, pcm_format()?))
            }
        } else {
            None
        };

        // SAFETY: all inputs have been added.
        unsafe {
            if !writer.startWriting() {
                return Err(writer_error(&writer, "could not start writing"));
            }
            writer.startSessionAtSourceTime(CMTime::new(0, 1));
        }

        Ok(Self {
            writer,
            video_input,
            adaptor,
            audio,
            finished: false,
        })
    }

    fn check_status(&self) -> AppResult<()> {
        // SAFETY: plain property read.
        if unsafe { self.writer.status() } == AVAssetWriterStatus::Failed {
            Err(writer_error(&self.writer, "writing failed"))
        } else {
            Ok(())
        }
    }

    fn new_pixel_buffer(&self) -> AppResult<CFRetained<CVPixelBuffer>> {
        // SAFETY: the pool exists once writing has started.
        let pool: Retained<CVPixelBufferPool> = unsafe { self.adaptor.pixelBufferPool() }
            .ok_or_else(|| writer_error(&self.writer, "no pixel buffer pool"))?;
        let mut buffer: *mut CVPixelBuffer = ptr::null_mut();
        // SAFETY: `buffer` receives a +1 pixel buffer on success.
        let status = unsafe {
            CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut buffer))
        };
        match NonNull::new(buffer) {
            // SAFETY: Create rule: we own the returned reference.
            Some(buffer) if status == 0 => Ok(unsafe { CFRetained::from_raw(buffer) }),
            _ => Err(AppError::Encoder(format!(
                "out of video buffers (status {status})"
            ))),
        }
    }
}

fn writer_error(writer: &AVAssetWriter, context: &str) -> AppError {
    // SAFETY: plain property read.
    let detail = unsafe { writer.error() }
        .map(|e| describe_error(&e))
        .unwrap_or_else(|| "unknown error".into());
    let lower = detail.to_lowercase();
    if lower.contains("disk") && (lower.contains("full") || lower.contains("space")) {
        return AppError::Storage(format!("the disk is full: {detail}"));
    }
    AppError::Encoder(format!("{context}: {detail}"))
}

impl VideoEncoder for MacEncoder {
    fn append_video(
        &mut self,
        media_time: f64,
        fill: &mut dyn FnMut(&mut BgraMut<'_>),
    ) -> AppResult<bool> {
        self.check_status()?;
        // SAFETY: plain property read.
        if !unsafe { self.video_input.isReadyForMoreMediaData() } {
            return Ok(false);
        }
        let buffer = self.new_pixel_buffer()?;
        if !write_pixel_buffer(&buffer, fill) {
            return Err(AppError::Encoder("could not access a video buffer".into()));
        }
        // SAFETY: the buffer is fully written and no longer modified.
        let appended = unsafe {
            self.adaptor
                .appendPixelBuffer_withPresentationTime(&buffer, cm_time(media_time))
        };
        if !appended {
            self.check_status()?;
        }
        Ok(appended)
    }

    fn append_audio(&mut self, media_time: f64, samples: &[f32]) -> AppResult<()> {
        let Some((input, format)) = &self.audio else {
            return Ok(());
        };
        if samples.is_empty() {
            return Ok(());
        }
        self.check_status()?;

        // Audio should never be dropped; wait briefly if the encoder is busy.
        let deadline = Instant::now() + Duration::from_millis(250);
        // SAFETY: plain property read.
        while !unsafe { input.isReadyForMoreMediaData() } {
            if Instant::now() > deadline {
                log::warn!("audio encoder busy; dropping {} samples", samples.len());
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }

        let sample_buffer = pcm_sample_buffer(format, media_time, samples)?;
        // SAFETY: the sample buffer is complete and ready.
        if !unsafe { input.appendSampleBuffer(&sample_buffer) } {
            self.check_status()?;
        }
        Ok(())
    }

    fn finish(&mut self, end_time: f64) -> AppResult<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.check_status()?;
        // SAFETY: ending the session and finishing inputs once.
        unsafe {
            self.writer.endSessionAtSourceTime(cm_time(end_time));
            self.video_input.markAsFinished();
            if let Some((input, _)) = &self.audio {
                input.markAsFinished();
            }
        }
        let writer = self.writer.clone();
        let done = wait_for(Duration::from_secs(60), move |tx| {
            let handler = RcBlock::new(move || {
                let _ = tx.send(());
            });
            // SAFETY: the block is 'static and called once.
            unsafe { writer.finishWritingWithCompletionHandler(&handler) };
        });
        if done.is_none() {
            return Err(AppError::Encoder(
                "timed out finalizing the MP4 file".into(),
            ));
        }
        // SAFETY: plain property read.
        match unsafe { self.writer.status() } {
            AVAssetWriterStatus::Completed => Ok(()),
            _ => Err(writer_error(&self.writer, "could not finalize the file")),
        }
    }
}

/// Wraps interleaved PCM in a `CMSampleBuffer` for the AAC encoder.
fn pcm_sample_buffer(
    format: &CMFormatDescription,
    media_time: f64,
    samples: &[f32],
) -> AppResult<CFRetained<CMSampleBuffer>> {
    let bytes = std::mem::size_of_val(samples);
    let frames = samples.len() / CHANNELS;
    let mut block: *mut CMBlockBuffer = ptr::null_mut();
    // SAFETY: CoreMedia allocates `bytes` of memory for the block buffer
    // (null memory block + default allocator) and we copy the samples into
    // it; every out-pointer is valid and results follow the Create rule.
    unsafe {
        let status = CMBlockBuffer::create_with_memory_block(
            None,
            ptr::null_mut(),
            bytes,
            None,
            ptr::null(),
            0,
            bytes,
            0,
            NonNull::from(&mut block),
        );
        let block = match NonNull::new(block) {
            Some(block) if status == 0 => CFRetained::from_raw(block),
            _ => {
                return Err(AppError::Encoder(format!(
                    "audio buffer allocation failed ({status})"
                )))
            }
        };
        let status = CMBlockBuffer::replace_data_bytes(
            NonNull::new_unchecked(samples.as_ptr().cast_mut().cast()),
            &block,
            0,
            bytes,
        );
        if status != 0 {
            return Err(AppError::Encoder(format!("audio copy failed ({status})")));
        }

        let mut sample_buffer: *mut CMSampleBuffer = ptr::null_mut();
        let status = CMAudioSampleBufferCreateReadyWithPacketDescriptions(
            None,
            &block,
            format,
            frames as isize,
            cm_time(media_time),
            ptr::null(),
            NonNull::from(&mut sample_buffer),
        );
        match NonNull::new(sample_buffer) {
            Some(buffer) if status == 0 => Ok(CFRetained::from_raw(buffer)),
            _ => Err(AppError::Encoder(format!(
                "audio sample buffer failed ({status})"
            ))),
        }
    }
}
