//! `CVPixelBuffer`-backed video frames.

use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    kCVPixelFormatType_32BGRA, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};

use crate::recording::capture::VideoFrame;
use crate::video::compositor::{BgraMut, BgraRef};

/// A captured BGRA frame, retained until every consumer is done with it.
pub struct PixelBufferFrame {
    buffer: CFRetained<CVPixelBuffer>,
}

// SAFETY: CVPixelBuffer is a reference-counted CoreFoundation object whose
// retain/release and lock/unlock operations are thread safe. We only read
// frames delivered by capture APIs, which do not modify them afterwards.
unsafe impl Send for PixelBufferFrame {}
unsafe impl Sync for PixelBufferFrame {}

impl PixelBufferFrame {
    /// Wraps a pixel buffer if it is in the 32-bit BGRA format we request.
    pub fn new(buffer: CFRetained<CVPixelBuffer>) -> Option<Self> {
        (CVPixelBufferGetPixelFormatType(&buffer) == kCVPixelFormatType_32BGRA)
            .then_some(Self { buffer })
    }
}

impl VideoFrame for PixelBufferFrame {
    fn width(&self) -> usize {
        CVPixelBufferGetWidth(&self.buffer)
    }

    fn height(&self) -> usize {
        CVPixelBufferGetHeight(&self.buffer)
    }

    fn read(&self, read: &mut dyn FnMut(BgraRef<'_>)) {
        let flags = CVPixelBufferLockFlags::ReadOnly;
        // SAFETY: the buffer is valid and locked while its memory is accessed;
        // the slice does not outlive the lock.
        unsafe {
            if CVPixelBufferLockBaseAddress(&self.buffer, flags) != 0 {
                return;
            }
            let base = CVPixelBufferGetBaseAddress(&self.buffer).cast::<u8>();
            if !base.is_null() {
                let stride = CVPixelBufferGetBytesPerRow(&self.buffer);
                let height = CVPixelBufferGetHeight(&self.buffer);
                read(BgraRef {
                    data: std::slice::from_raw_parts(base, stride * height),
                    width: CVPixelBufferGetWidth(&self.buffer),
                    height,
                    stride,
                });
            }
            CVPixelBufferUnlockBaseAddress(&self.buffer, flags);
        }
    }
}

/// Locks a pixel buffer for writing and hands its memory to `write`.
pub fn write_pixel_buffer(buffer: &CVPixelBuffer, write: &mut dyn FnMut(&mut BgraMut<'_>)) -> bool {
    let flags = CVPixelBufferLockFlags::empty();
    // SAFETY: the buffer is locked for writing while its memory is accessed
    // and the slice does not outlive the lock.
    unsafe {
        if CVPixelBufferLockBaseAddress(buffer, flags) != 0 {
            return false;
        }
        let base = CVPixelBufferGetBaseAddress(buffer).cast::<u8>();
        let ok = !base.is_null();
        if ok {
            let stride = CVPixelBufferGetBytesPerRow(buffer);
            let height = CVPixelBufferGetHeight(buffer);
            write(&mut BgraMut {
                data: std::slice::from_raw_parts_mut(base, stride * height),
                width: CVPixelBufferGetWidth(buffer),
                height,
                stride,
            });
        }
        CVPixelBufferUnlockBaseAddress(buffer, flags);
        ok
    }
}
