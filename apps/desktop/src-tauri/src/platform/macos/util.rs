//! Small helpers shared by the macOS backend.

use std::ptr::{self, NonNull};
use std::sync::mpsc;
use std::time::Duration;

use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved, kAudioFormatLinearPCM, AudioBuffer,
    AudioBufferList,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
    CMAudioFormatDescriptionGetStreamBasicDescription, CMBlockBuffer, CMClock, CMSampleBuffer,
    CMTime,
};
use objc2_foundation::{NSError, NSOperatingSystemVersion, NSProcessInfo};

use crate::audio::convert::{decode_samples, interleave, InputFormat, SampleFormat};
use crate::capabilities::OsVersion;

/// Runs `start`, which must eventually send exactly one value through the
/// given sender (typically from an Objective-C completion handler), and
/// waits for it.
pub fn wait_for<T: Send + 'static>(
    timeout: Duration,
    start: impl FnOnce(mpsc::Sender<T>),
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    start(tx);
    rx.recv_timeout(timeout).ok()
}

/// Reads an optional `NSError` passed to a completion handler.
///
/// # Safety
/// `error` must be null or point to a valid `NSError`.
pub unsafe fn error_message(error: *mut NSError) -> Option<(isize, String)> {
    // SAFETY: guaranteed by the caller.
    let error = unsafe { error.as_ref() }?;
    Some((error.code(), error.localizedDescription().to_string()))
}

/// Formats an error with its code and underlying causes, which carry the
/// actionable detail for AVFoundation's generic errors.
pub fn describe_error(error: &NSError) -> String {
    let mut message = format!("{} (code {})", error.localizedDescription(), error.code());
    for cause in error.underlyingErrors().iter() {
        message.push_str(&format!(
            "; {} {} (code {})",
            cause.domain(),
            cause.localizedDescription(),
            cause.code()
        ));
    }
    message
}

pub fn host_time() -> f64 {
    // SAFETY: the host time clock is a process-wide singleton.
    unsafe { CMClock::host_time_clock().time().seconds() }
}

pub fn cm_time(seconds: f64) -> CMTime {
    // A timescale divisible by common frame rates (24, 25, 30, 60) and by
    // 48 kHz audio blocks keeps timestamps exact.
    // SAFETY: plain value construction.
    unsafe { CMTime::with_seconds(seconds, 48_000) }
}

pub fn os_version() -> OsVersion {
    let NSOperatingSystemVersion {
        majorVersion,
        minorVersion,
        patchVersion,
    } = NSProcessInfo::processInfo().operatingSystemVersion();
    OsVersion::new(
        majorVersion as u32,
        minorVersion as u32,
        patchVersion as u32,
    )
}

pub fn is_at_least(major: u32) -> bool {
    os_version().major >= major
}

/// Extracts the PCM samples of an audio `CMSampleBuffer` as interleaved `f32`.
pub fn audio_samples(sample_buffer: &CMSampleBuffer) -> Option<(InputFormat, Vec<f32>)> {
    // SAFETY: the sample buffer is valid for the duration of this call; the
    // format description pointer is owned by the format description, which
    // we keep alive while reading it.
    let format = unsafe { sample_buffer.format_description() }?;
    let asbd = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&format).as_ref() }?;
    if asbd.mFormatID != kAudioFormatLinearPCM {
        return None;
    }
    let sample_format = if asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0 {
        match asbd.mBitsPerChannel {
            32 => SampleFormat::F32,
            _ => return None,
        }
    } else {
        match asbd.mBitsPerChannel {
            16 => SampleFormat::I16,
            32 => SampleFormat::I32,
            _ => return None,
        }
    };
    let input = InputFormat {
        sample_rate: asbd.mSampleRate.round() as u32,
        channels: asbd.mChannelsPerFrame as usize,
        format: sample_format,
    };
    let non_interleaved = asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0;

    let buffers = audio_buffers(sample_buffer)?;
    let samples = if non_interleaved && buffers.len() > 1 {
        let planes: Vec<Vec<f32>> = buffers
            .iter()
            .map(|b| decode_samples(b, sample_format))
            .collect();
        interleave(&planes)
    } else {
        buffers
            .first()
            .map(|b| decode_samples(b, sample_format))
            .unwrap_or_default()
    };
    Some((input, samples))
}

/// Copies the raw bytes of every `AudioBuffer` in a sample buffer.
fn audio_buffers(sample_buffer: &CMSampleBuffer) -> Option<Vec<Vec<u8>>> {
    let mut size_needed = 0usize;
    // SAFETY: querying the required size with a null list is documented usage.
    unsafe {
        sample_buffer.audio_buffer_list_with_retained_block_buffer(
            &mut size_needed,
            ptr::null_mut(),
            0,
            None,
            None,
            0,
            ptr::null_mut(),
        );
    }
    if size_needed == 0 {
        return None;
    }

    // u64 storage keeps the list suitably aligned.
    let mut storage = vec![0u64; size_needed.div_ceil(8)];
    let list = storage.as_mut_ptr().cast::<AudioBufferList>();
    let mut block: *mut CMBlockBuffer = ptr::null_mut();
    // SAFETY: `list` points to at least `size_needed` bytes; on success
    // CoreMedia fills it and returns a retained block buffer that owns the
    // sample memory, which we release after copying.
    let status = unsafe {
        sample_buffer.audio_buffer_list_with_retained_block_buffer(
            ptr::null_mut(),
            list,
            size_needed,
            None,
            None,
            kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
            &mut block,
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the block buffer was returned retained ("Create rule").
    let _block = NonNull::new(block).map(|b| unsafe { CFRetained::from_raw(b) });

    // SAFETY: CoreMedia initialized `mNumberBuffers` buffers in the list.
    let buffers = unsafe {
        let count = (*list).mNumberBuffers as usize;
        let first = ptr::addr_of!((*list).mBuffers).cast::<AudioBuffer>();
        (0..count)
            .map(|i| {
                let buffer = &*first.add(i);
                if buffer.mData.is_null() {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(
                        buffer.mData.cast::<u8>(),
                        buffer.mDataByteSize as usize,
                    )
                    .to_vec()
                }
            })
            .collect()
    };
    Some(buffers)
}
