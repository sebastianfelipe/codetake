//! A running ScreenCaptureKit stream delivering video or audio to a sink.

use std::sync::Arc;
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_media::CMSampleBuffer;
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput,
    SCStreamOutputType,
};
use parking_lot::Mutex;

use super::frame::PixelBufferFrame;
use super::util::{audio_samples, describe_error, error_message, wait_for};
use crate::audio::convert::Converter;
use crate::error::AppError;
use crate::permissions::PermissionKind;
use crate::recording::capture::{AudioSink, VideoSink};

/// ScreenCaptureKit's "user declined" error (`SCStreamErrorUserDeclined`).
const USER_DECLINED: isize = -3801;

/// Where a stream delivers its samples.
pub enum StreamTarget {
    Video(Arc<dyn VideoSink>),
    Audio(Arc<dyn AudioSink>),
}

impl StreamTarget {
    fn error(&self, error: AppError) {
        match self {
            StreamTarget::Video(sink) => sink.error(error),
            StreamTarget::Audio(sink) => sink.error(error),
        }
    }
}

pub struct OutputIvars {
    target: StreamTarget,
    converter: Mutex<Option<Converter>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and we don't implement Drop.
    #[unsafe(super(NSObject))]
    #[ivars = OutputIvars]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(
            &self,
            _stream: &SCStream,
            sample_buffer: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            self.handle(sample_buffer, kind);
        }
    }

    unsafe impl SCStreamDelegate for StreamOutput {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            self.ivars().target.error(stream_error(error));
        }
    }
);

impl StreamOutput {
    fn new(target: StreamTarget) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars {
            target,
            converter: Mutex::new(None),
        });
        // SAFETY: NSObject's init is always safe to call on a fresh allocation.
        unsafe { msg_send![super(this), init] }
    }

    fn handle(&self, sample_buffer: &CMSampleBuffer, kind: SCStreamOutputType) {
        // SAFETY: the sample buffer is valid for the duration of the callback.
        let host_time = unsafe { sample_buffer.presentation_time_stamp().seconds() };
        match (&self.ivars().target, kind) {
            (StreamTarget::Video(sink), SCStreamOutputType::Screen) => {
                // Idle frames (nothing changed on screen) carry no image.
                // SAFETY: as above.
                let Some(image) = (unsafe { sample_buffer.image_buffer() }) else {
                    return;
                };
                if let Some(frame) = PixelBufferFrame::new(image) {
                    sink.frame(Arc::new(frame), host_time);
                }
            }
            (StreamTarget::Audio(sink), SCStreamOutputType::Audio) => {
                let Some((format, samples)) = audio_samples(sample_buffer) else {
                    return;
                };
                let mut converter = self.ivars().converter.lock();
                let converter = match converter.as_mut() {
                    Some(c) if c.input_format() == format => c,
                    _ => converter.insert(Converter::new(format)),
                };
                sink.audio(host_time, &converter.convert(&samples));
            }
            _ => {}
        }
    }
}

fn stream_error(error: &NSError) -> AppError {
    if error.code() == USER_DECLINED {
        AppError::PermissionDenied(PermissionKind::ScreenRecording)
    } else {
        AppError::ScreenCapture(describe_error(error))
    }
}

/// An `SCStream` with its output object and callback queue.
pub struct RunningStream {
    stream: Retained<SCStream>,
    _output: Retained<StreamOutput>,
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: SCStream's start/stop methods may be called from any thread; the
// output object is only accessed by ScreenCaptureKit on its queue.
unsafe impl Send for RunningStream {}

impl RunningStream {
    pub fn start(
        filter: &SCContentFilter,
        config: &SCStreamConfiguration,
        target: StreamTarget,
        label: &str,
    ) -> Result<Self, AppError> {
        let kind = match target {
            StreamTarget::Video(_) => SCStreamOutputType::Screen,
            StreamTarget::Audio(_) => SCStreamOutputType::Audio,
        };
        let output = StreamOutput::new(target);
        let queue = DispatchQueue::new(label, None);
        let delegate = ProtocolObject::from_ref(&*output);

        // SAFETY: filter/config are valid; the delegate and output are kept
        // alive by `RunningStream` for as long as the stream exists.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                filter,
                config,
                Some(delegate),
            )
        };
        // SAFETY: as above.
        unsafe {
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    kind,
                    Some(&queue),
                )
                .map_err(|e| stream_error(&e))?;
        }

        let result = wait_for(Duration::from_secs(10), |tx| {
            let handler = RcBlock::new(move |error: *mut NSError| {
                // SAFETY: ScreenCaptureKit passes null or a valid error.
                let _ = tx.send(unsafe { error_message(error) });
            });
            // SAFETY: the block is 'static and called once.
            unsafe { stream.startCaptureWithCompletionHandler(Some(&handler)) };
        });
        match result {
            Some(None) => Ok(Self {
                stream,
                _output: output,
                _queue: queue,
            }),
            Some(Some((code, _))) if code == USER_DECLINED => {
                Err(AppError::PermissionDenied(PermissionKind::ScreenRecording))
            }
            Some(Some((code, message))) => {
                Err(AppError::ScreenCapture(format!("{message} (code {code})")))
            }
            None => Err(AppError::ScreenCapture(
                "timed out starting screen capture".into(),
            )),
        }
    }

    pub fn stop(self) {
        let _ = wait_for(Duration::from_secs(5), |tx| {
            let handler = RcBlock::new(move |_error: *mut NSError| {
                let _ = tx.send(());
            });
            // SAFETY: the block is 'static and called once.
            unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&handler)) };
        });
    }
}
