//! Decodes audio files (AAC, MP3, WAV, ...) with AudioToolbox's ExtAudioFile.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{self, NonNull};

use objc2_audio_toolbox::{
    kExtAudioFileProperty_ClientDataFormat, ExtAudioFileDispose, ExtAudioFileOpenURL,
    ExtAudioFileRead, ExtAudioFileRef, ExtAudioFileSetProperty,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, AudioBuffer,
    AudioBufferList, AudioStreamBasicDescription,
};
use objc2_core_foundation::CFURL;
use objc2_foundation::{NSString, NSURL};

use crate::audio::{CHANNELS, SAMPLE_RATE};
use crate::error::{AppError, AppResult};

/// Closes the file when dropped.
struct ExtAudioFile(ExtAudioFileRef);

impl Drop for ExtAudioFile {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by ExtAudioFileOpenURL and is disposed once.
        unsafe { ExtAudioFileDispose(self.0) };
    }
}

/// Reads the whole file as 48 kHz interleaved stereo `f32`.
pub fn decode(path: &Path) -> AppResult<Vec<f32>> {
    let fail = |what: &str, status: i32| {
        AppError::Music(format!("{what} {} (status {status})", path.display()))
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // SAFETY: NSURL is toll-free bridged with CFURL.
    let cf_url: &CFURL = unsafe { &*(&*url as *const NSURL).cast::<CFURL>() };

    let mut handle: ExtAudioFileRef = ptr::null_mut();
    // SAFETY: valid URL and out-pointer.
    let status = unsafe { ExtAudioFileOpenURL(cf_url, NonNull::from(&mut handle)) };
    if status != 0 || handle.is_null() {
        return Err(fail("cannot open", status));
    }
    let file = ExtAudioFile(handle);

    // Ask ExtAudioFile to decode and convert to the mixer format for us.
    let bytes_per_frame = (CHANNELS * std::mem::size_of::<f32>()) as u32;
    let client = AudioStreamBasicDescription {
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
    // SAFETY: `client` is a valid ASBD of the given size.
    let status = unsafe {
        ExtAudioFileSetProperty(
            file.0,
            kExtAudioFileProperty_ClientDataFormat,
            std::mem::size_of::<AudioStreamBasicDescription>() as u32,
            NonNull::from(&client).cast::<c_void>(),
        )
    };
    if status != 0 {
        return Err(fail("cannot decode", status));
    }

    const CHUNK_FRAMES: usize = 8192;
    let mut chunk = vec![0.0_f32; CHUNK_FRAMES * CHANNELS];
    let mut samples = Vec::new();
    loop {
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: CHANNELS as u32,
                mDataByteSize: (chunk.len() * std::mem::size_of::<f32>()) as u32,
                mData: chunk.as_mut_ptr().cast(),
            }],
        };
        let mut frames = CHUNK_FRAMES as u32;
        // SAFETY: `list` describes `chunk`, which outlives the call.
        let status = unsafe {
            ExtAudioFileRead(file.0, NonNull::from(&mut frames), NonNull::from(&mut list))
        };
        if status != 0 {
            return Err(fail("error while decoding", status));
        }
        if frames == 0 {
            break;
        }
        samples.extend_from_slice(&chunk[..frames as usize * CHANNELS]);
    }
    if samples.is_empty() {
        return Err(AppError::Music(format!(
            "{} contains no audio",
            path.display()
        )));
    }
    Ok(samples)
}
