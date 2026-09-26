//! Camera and microphone capture with `AVCaptureSession`.

use std::sync::Arc;

use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVCaptureAudioDataOutput, AVCaptureAudioDataOutputSampleBufferDelegate, AVCaptureConnection,
    AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession, AVCaptureSessionPreset1280x720,
    AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate,
};
use objc2_avf_audio::{
    AVFormatIDKey, AVLinearPCMBitDepthKey, AVLinearPCMIsBigEndianKey, AVLinearPCMIsFloatKey,
    AVLinearPCMIsNonInterleaved, AVNumberOfChannelsKey, AVSampleRateKey,
};
use objc2_core_audio_types::kAudioFormatLinearPCM;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use parking_lot::Mutex;

use super::devices::capture_device;
use super::frame::PixelBufferFrame;
use super::permissions;
use super::util::{audio_samples, describe_error};
use crate::audio::convert::Converter;
use crate::error::{AppError, AppResult};
use crate::permissions::PermissionKind;
use crate::recording::capture::{AudioSink, CameraCapture, MicrophoneCapture, VideoSink};

pub struct DelegateIvars {
    video: Option<Arc<dyn VideoSink>>,
    audio: Option<Arc<dyn AudioSink>>,
    converter: Mutex<Option<Converter>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and we don't implement Drop.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    struct SampleDelegate;

    unsafe impl NSObjectProtocol for SampleDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for SampleDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            self.handle(sample_buffer);
        }
    }

    unsafe impl AVCaptureAudioDataOutputSampleBufferDelegate for SampleDelegate {}
);

impl SampleDelegate {
    fn new(video: Option<Arc<dyn VideoSink>>, audio: Option<Arc<dyn AudioSink>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            video,
            audio,
            converter: Mutex::new(None),
        });
        // SAFETY: NSObject's init is always safe to call on a fresh allocation.
        unsafe { msg_send![super(this), init] }
    }

    fn handle(&self, sample_buffer: &CMSampleBuffer) {
        let ivars = self.ivars();
        // SAFETY: the sample buffer is valid for the duration of the callback.
        let host_time = unsafe { sample_buffer.presentation_time_stamp().seconds() };
        if let Some(sink) = &ivars.video {
            // SAFETY: as above.
            if let Some(frame) =
                unsafe { sample_buffer.image_buffer() }.and_then(PixelBufferFrame::new)
            {
                sink.frame(Arc::new(frame), host_time);
            }
        }
        if let Some(sink) = &ivars.audio {
            if let Some((format, samples)) = audio_samples(sample_buffer) {
                let mut converter = ivars.converter.lock();
                let converter = match converter.as_mut() {
                    Some(c) if c.input_format() == format => c,
                    _ => converter.insert(Converter::new(format)),
                };
                sink.audio(host_time, &converter.convert(&samples));
            }
        }
    }
}

/// A running capture session with its delegate and callback queue.
struct Session {
    session: Retained<AVCaptureSession>,
    _delegate: Retained<SampleDelegate>,
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: AVCaptureSession may be started and stopped from any thread; the
// delegate is only called by AVFoundation on its queue.
unsafe impl Send for Session {}

impl Session {
    fn stop(self) {
        // SAFETY: stopping a running session is always valid.
        unsafe { self.session.stopRunning() };
    }
}

fn dictionary(
    entries: &[(&NSString, Retained<AnyObject>)],
) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = entries.iter().map(|(k, _)| *k).collect();
    let values: Vec<Retained<AnyObject>> = entries.iter().map(|(_, v)| v.clone()).collect();
    NSDictionary::from_retained_objects(&keys, &values)
}

fn number(value: f64) -> Retained<AnyObject> {
    NSNumber::new_f64(value).into()
}

fn integer(value: i64) -> Retained<AnyObject> {
    NSNumber::new_i64(value).into()
}

fn boolean(value: bool) -> Retained<AnyObject> {
    NSNumber::new_bool(value).into()
}

fn start_session(
    device_id: &str,
    kind: PermissionKind,
    configure: impl FnOnce(
        &AVCaptureSession,
        &Retained<SampleDelegate>,
        &DispatchQueue,
    ) -> AppResult<()>,
    delegate: Retained<SampleDelegate>,
    label: &str,
) -> AppResult<Session> {
    permissions::ensure(kind)?;
    let make_error = |message: String| match kind {
        PermissionKind::Camera => AppError::Camera(message),
        _ => AppError::Microphone(message),
    };
    let device = capture_device(device_id).ok_or_else(|| {
        AppError::DeviceUnavailable(match kind {
            PermissionKind::Camera => "the selected camera is not connected".into(),
            _ => "the selected microphone is not connected".into(),
        })
    })?;
    // SAFETY: `device` is a valid capture device.
    let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
        .map_err(|e| make_error(describe_error(&e)))?;

    let queue = DispatchQueue::new(label, None);
    // SAFETY: configuring a fresh session between begin/commitConfiguration.
    let session = unsafe {
        let session = AVCaptureSession::new();
        session.beginConfiguration();
        if !session.canAddInput(&input) {
            session.commitConfiguration();
            return Err(make_error("the device cannot be used for capture".into()));
        }
        session.addInput(&input);
        let configured = configure(&session, &delegate, &queue);
        session.commitConfiguration();
        configured?;
        session
    };
    // SAFETY: starting a configured session. This blocks until running.
    unsafe { session.startRunning() };
    // SAFETY: plain property read.
    if !unsafe { session.isRunning() } {
        return Err(make_error(
            "the device could not be started (it may be in use by another app)".into(),
        ));
    }
    Ok(Session {
        session,
        _delegate: delegate,
        _queue: queue,
    })
}

pub struct MacCameraCapture {
    device_id: String,
    session: Option<Session>,
}

impl MacCameraCapture {
    pub fn new(device_id: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            session: None,
        }
    }
}

impl CameraCapture for MacCameraCapture {
    fn start(&mut self, sink: Arc<dyn VideoSink>) -> AppResult<()> {
        let delegate = SampleDelegate::new(Some(sink), None);
        let session = start_session(
            &self.device_id,
            PermissionKind::Camera,
            |session, delegate, queue| {
                // SAFETY: configuring objects owned by this session.
                unsafe {
                    let preset = AVCaptureSessionPreset1280x720;
                    if session.canSetSessionPreset(preset) {
                        session.setSessionPreset(preset);
                    }
                    let output = AVCaptureVideoDataOutput::new();
                    let settings = dictionary(&[(
                        kCVPixelBufferPixelFormatTypeKey.as_ref(),
                        integer(i64::from(kCVPixelFormatType_32BGRA)),
                    )]);
                    output.setVideoSettings(Some(&settings));
                    output.setAlwaysDiscardsLateVideoFrames(true);
                    output.setSampleBufferDelegate_queue(
                        Some(ProtocolObject::from_ref(&**delegate)),
                        Some(queue),
                    );
                    if !session.canAddOutput(&output) {
                        return Err(AppError::Camera(
                            "cannot read frames from this camera".into(),
                        ));
                    }
                    session.addOutput(&output);
                }
                Ok(())
            },
            delegate,
            "dev.codetake.capture.camera",
        )?;
        self.session = Some(session);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            session.stop();
        }
    }
}

impl Drop for MacCameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct MacMicrophoneCapture {
    device_id: String,
    session: Option<Session>,
}

impl MacMicrophoneCapture {
    pub fn new(device_id: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            session: None,
        }
    }
}

fn required(key: Option<&'static NSString>) -> AppResult<&'static NSString> {
    key.ok_or_else(|| AppError::Microphone("AVFoundation audio settings are unavailable".into()))
}

impl MicrophoneCapture for MacMicrophoneCapture {
    fn start(&mut self, sink: Arc<dyn AudioSink>) -> AppResult<()> {
        let delegate = SampleDelegate::new(None, Some(sink));
        let session = start_session(
            &self.device_id,
            PermissionKind::Microphone,
            |session, delegate, queue| {
                // Ask AVFoundation for the mixer format directly (48 kHz
                // interleaved stereo float) so no resampling is needed.
                // SAFETY: reading framework constants and configuring objects
                // owned by this session.
                unsafe {
                    let settings = dictionary(&[
                        (
                            required(AVFormatIDKey)?,
                            integer(i64::from(kAudioFormatLinearPCM)),
                        ),
                        (required(AVSampleRateKey)?, number(48_000.0)),
                        (required(AVNumberOfChannelsKey)?, integer(2)),
                        (required(AVLinearPCMBitDepthKey)?, integer(32)),
                        (required(AVLinearPCMIsFloatKey)?, boolean(true)),
                        (required(AVLinearPCMIsNonInterleaved)?, boolean(false)),
                        (required(AVLinearPCMIsBigEndianKey)?, boolean(false)),
                    ]);
                    let output = AVCaptureAudioDataOutput::new();
                    output.setAudioSettings(Some(&settings));
                    output.setSampleBufferDelegate_queue(
                        Some(ProtocolObject::from_ref(&**delegate)),
                        Some(queue),
                    );
                    if !session.canAddOutput(&output) {
                        return Err(AppError::Microphone(
                            "cannot read audio from this microphone".into(),
                        ));
                    }
                    session.addOutput(&output);
                }
                Ok(())
            },
            delegate,
            "dev.codetake.capture.microphone",
        )?;
        self.session = Some(session);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            session.stop();
        }
    }
}

impl Drop for MacMicrophoneCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
