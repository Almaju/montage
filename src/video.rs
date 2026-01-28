use anyhow::{Context as AnyhowContext, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gpui::*;
use image::{ImageBuffer, Rgba};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Video frame data ready for display
#[allow(dead_code)]
#[derive(Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub height: u32,
    pub width: u32,
}

#[allow(dead_code)]
impl VideoFrame {
    /// Convert frame data to an image::Frame for gpui rendering
    pub fn to_image_frame(&self) -> Option<image::Frame> {
        let img_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> = 
            ImageBuffer::from_raw(self.width, self.height, self.data.clone())?;
        Some(image::Frame::new(img_buffer))
    }
    
    /// Create a RenderImage for use with gpui's img() element
    pub fn to_render_image(&self) -> Option<Arc<RenderImage>> {
        let frame = self.to_image_frame()?;
        Some(Arc::new(RenderImage::new(vec![frame])))
    }
}

/// Video player using GStreamer
#[allow(dead_code)]
pub struct VideoPlayer {
    /// Current frame to display
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    /// Video duration in seconds
    duration: f64,
    /// GStreamer pipeline
    pipeline: Option<gst::Pipeline>,
    /// Video dimensions
    video_height: u32,
    video_width: u32,
}

#[allow(dead_code)]
impl VideoPlayer {
    pub fn new() -> Self {
        // Initialize GStreamer
        gst::init().expect("Failed to initialize GStreamer");

        Self {
            current_frame: Arc::new(Mutex::new(None)),
            duration: 0.0,
            pipeline: None,
            video_height: 720,
            video_width: 1280,
        }
    }

    /// Load a video file
    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        let uri = format!("file://{}", path.canonicalize()?.display());

        // Create pipeline: filesrc -> decodebin -> videoconvert -> appsink
        let pipeline = gst::Pipeline::new();

        let src = gst::ElementFactory::make("uridecodebin")
            .property("uri", &uri)
            .build()
            .context("Failed to create uridecodebin")?;

        let convert = gst::ElementFactory::make("videoconvert")
            .build()
            .context("Failed to create videoconvert")?;

        let sink = gst_app::AppSink::builder()
            .caps(
                &gst_video::VideoCapsBuilder::new()
                    .format(gst_video::VideoFormat::Rgba)
                    .build(),
            )
            .build();

        pipeline.add_many([&src, &convert, sink.upcast_ref()])?;
        gst::Element::link_many([&convert, sink.upcast_ref()])?;

        // Handle dynamic pad linking for decodebin
        let convert_weak = convert.downgrade();
        src.connect_pad_added(move |_, src_pad| {
            let Some(convert) = convert_weak.upgrade() else {
                return;
            };

            let sink_pad = convert.static_pad("sink").unwrap();
            if sink_pad.is_linked() {
                return;
            }

            let caps = src_pad.current_caps().unwrap();
            let structure = caps.structure(0).unwrap();
            let name = structure.name();

            if name.starts_with("video/") {
                src_pad.link(&sink_pad).unwrap();
            }
        });

        // Set up frame capture
        let frame_ref = self.current_frame.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    let video_info = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let width = video_info.width();
                    let height = video_info.height();

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let data = map.as_slice().to_vec();

                    let frame = VideoFrame {
                        data,
                        height,
                        width,
                    };

                    *frame_ref.lock().unwrap() = Some(frame);

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // Start pipeline in paused state
        pipeline.set_state(gst::State::Paused)?;

        // Wait for preroll and get duration
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));

        if let Some(duration) = pipeline.query_duration::<gst::ClockTime>() {
            self.duration = duration.seconds() as f64;
        }

        // Get video dimensions from caps
        if let Some(pad) = convert.static_pad("sink")
            && let Some(caps) = pad.current_caps()
            && let Ok(video_info) = gst_video::VideoInfo::from_caps(&caps)
        {
            self.video_height = video_info.height();
            self.video_width = video_info.width();
        }

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Seek to a normalized position (0.0 to 1.0)
    pub fn seek(&self, position: f64) {
        if let Some(ref pipeline) = self.pipeline {
            let position_ns = (position * self.duration * 1_000_000_000.0) as u64;
            let _ = pipeline.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_nseconds(position_ns),
            );
        }
    }

    /// Get current frame for display
    pub fn current_frame(&self) -> Option<VideoFrame> {
        self.current_frame.lock().unwrap().clone()
    }

    /// Get video duration in seconds
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Get video dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.video_width, self.video_height)
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        if let Some(ref pipeline) = self.pipeline {
            let _ = pipeline.set_state(gst::State::Null);
        }
    }
}

/// Video preview component for gpui
#[allow(dead_code)]
pub struct VideoPreview {
    frame: Option<VideoFrame>,
}

#[allow(dead_code)]
impl VideoPreview {
    pub fn new() -> Self {
        Self { frame: None }
    }

    pub fn set_frame(&mut self, frame: Option<VideoFrame>) {
        self.frame = frame;
    }
}

impl Render for VideoPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h_96()
            .bg(rgb(0x000000))
            .rounded_lg()
            .flex()
            .items_center()
            .justify_center()
            .child(if self.frame.is_some() {
                // TODO: Render actual frame using gpui's image rendering
                // For now, show a placeholder indicating video is loaded
                div()
                    .text_color(rgb(0x4fc3f7))
                    .child("🎬 Video loaded")
                    .into_any_element()
            } else {
                div()
                    .text_color(rgb(0x666666))
                    .child("No video loaded")
                    .into_any_element()
            })
    }
}
