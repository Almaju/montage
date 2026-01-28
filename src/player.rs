use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::project::Project;

/// Frame data for display
#[derive(Clone)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Player state
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerState {
    Stopped,
    Paused,
    Playing,
}

/// Unified project player - same pipeline for preview and export
#[allow(dead_code)]
pub struct ProjectPlayer {
    /// Current frame for display
    current_frame: Arc<Mutex<Option<Frame>>>,
    /// GStreamer pipeline
    pipeline: Option<gst::Pipeline>,
    /// Player state
    state: PlayerState,
    /// Total duration in seconds
    duration: f64,
    /// Current position in seconds
    position: f64,
    /// Video dimensions
    width: u32,
    height: u32,
}

impl ProjectPlayer {
    pub fn new() -> Self {
        Self {
            current_frame: Arc::new(Mutex::new(None)),
            pipeline: None,
            state: PlayerState::Stopped,
            duration: 0.0,
            position: 0.0,
            width: 1280,
            height: 720,
        }
    }
    
    /// Build and load the project pipeline
    pub fn load_project(&mut self, project: &Project) -> Result<()> {
        // Clean up old pipeline
        self.stop();
        
        // Get video clips
        let video_clips: Vec<PathBuf> = project.clips
            .iter()
            .filter(|c| c.media_type == crate::project::MediaType::Video)
            .map(|c| c.path.clone())
            .collect();
        
        if video_clips.is_empty() {
            tracing::info!("No video clips to play");
            return Ok(());
        }
        
        // Get audio track
        let audio_track = project.audio.as_ref().map(|a| a.path.clone());
        
        tracing::info!("Building player: {} videos, audio: {:?}", video_clips.len(), audio_track.is_some());
        
        // Build the pipeline
        self.build_pipeline(&video_clips, audio_track.as_ref())?;
        
        Ok(())
    }
    
    /// Build GStreamer pipeline for playback
    fn build_pipeline(&mut self, video_clips: &[PathBuf], audio_track: Option<&PathBuf>) -> Result<()> {
        let pipeline = gst::Pipeline::new();
        
        // For single video, simple pipeline
        if video_clips.len() == 1 {
            self.build_single_video_pipeline(&pipeline, &video_clips[0], audio_track)?;
        } else {
            // For multiple videos, use concat
            self.build_concat_pipeline(&pipeline, video_clips, audio_track)?;
        }
        
        // Start in paused state
        pipeline.set_state(gst::State::Paused)?;
        
        // Wait for preroll
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));
        
        // Get duration
        if let Some(dur) = pipeline.query_duration::<gst::ClockTime>() {
            self.duration = dur.nseconds() as f64 / 1_000_000_000.0;
        }
        
        self.pipeline = Some(pipeline);
        self.state = PlayerState::Paused;
        
        Ok(())
    }
    
    /// Build pipeline for single video
    fn build_single_video_pipeline(
        &mut self,
        pipeline: &gst::Pipeline,
        video_path: &std::path::Path,
        audio_track: Option<&PathBuf>,
    ) -> Result<()> {
        let video_uri = format!("file://{}", video_path.canonicalize()?.display());
        
        // Video decode -> convert -> appsink (for preview)
        let video_src = gst::ElementFactory::make("uridecodebin")
            .name("video_src")
            .property("uri", &video_uri)
            .build()?;
        
        let video_convert = gst::ElementFactory::make("videoconvert").build()?;
        let video_scale = gst::ElementFactory::make("videoscale").build()?;
        
        // Create tee to split video for preview
        let video_tee = gst::ElementFactory::make("tee").build()?;
        
        // Preview sink
        let preview_queue = gst::ElementFactory::make("queue").build()?;
        let preview_sink = gst_app::AppSink::builder()
            .name("preview_sink")
            .caps(&gst_video::VideoCapsBuilder::new()
                .format(gst_video::VideoFormat::Rgba)
                .build())
            .build();
        
        // Audio elements
        let audio_convert = gst::ElementFactory::make("audioconvert").build()?;
        let audio_resample = gst::ElementFactory::make("audioresample").build()?;
        let audio_sink = gst::ElementFactory::make("autoaudiosink").build()?;
        
        // Add video elements
        pipeline.add_many([
            &video_src, &video_convert, &video_scale, &video_tee,
            &preview_queue, preview_sink.upcast_ref::<gst::Element>(),
        ])?;
        
        // Add audio elements  
        pipeline.add_many([&audio_convert, &audio_resample, &audio_sink])?;
        
        // Link video chain
        gst::Element::link_many([&video_convert, &video_scale, &video_tee])?;
        video_tee.link(&preview_queue)?;
        preview_queue.link(preview_sink.upcast_ref::<gst::Element>())?;
        
        // Link audio chain
        gst::Element::link_many([&audio_convert, &audio_resample, &audio_sink])?;
        
        // Handle dynamic pads from uridecodebin
        let video_convert_weak = video_convert.downgrade();
        let audio_convert_weak = audio_convert.downgrade();
        
        video_src.connect_pad_added(move |_, pad| {
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            let structure = caps.structure(0).unwrap();
            let name = structure.name();
            
            if name.starts_with("video/")
                && let Some(convert) = video_convert_weak.upgrade()
            {
                let sink_pad = convert.static_pad("sink").unwrap();
                if !sink_pad.is_linked() {
                    let _ = pad.link(&sink_pad);
                }
            } else if name.starts_with("audio/")
                && let Some(convert) = audio_convert_weak.upgrade()
            {
                let sink_pad = convert.static_pad("sink").unwrap();
                if !sink_pad.is_linked() {
                    let _ = pad.link(&sink_pad);
                }
            }
        });
        
        // If we have a separate audio track (voiceover), add it
        if let Some(audio_path) = audio_track {
            // TODO: Mix voiceover with video audio
            // For now, voiceover will be used in export only
            tracing::info!("Voiceover track: {:?} (will be used in export)", audio_path);
        }
        
        // Set up frame callback
        let frame_ref = self.current_frame.clone();
        preview_sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                    
                    let video_info = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;
                    
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    
                    let frame = Frame {
                        data: map.as_slice().to_vec(),
                        width: video_info.width(),
                        height: video_info.height(),
                    };
                    
                    *frame_ref.lock().unwrap() = Some(frame);
                    
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        
        Ok(())
    }
    
    /// Build pipeline for multiple videos (concat)
    fn build_concat_pipeline(
        &mut self,
        pipeline: &gst::Pipeline,
        video_clips: &[PathBuf],
        _audio_track: Option<&PathBuf>,
    ) -> Result<()> {
        // For multiple clips, we need concat elements
        let video_concat = gst::ElementFactory::make("concat")
            .name("video_concat")
            .build()?;
        let audio_concat = gst::ElementFactory::make("concat")
            .name("audio_concat")
            .build()?;
        
        let video_convert = gst::ElementFactory::make("videoconvert").build()?;
        let video_scale = gst::ElementFactory::make("videoscale").build()?;
        
        let preview_sink = gst_app::AppSink::builder()
            .name("preview_sink")
            .caps(&gst_video::VideoCapsBuilder::new()
                .format(gst_video::VideoFormat::Rgba)
                .build())
            .build();
        
        let audio_convert = gst::ElementFactory::make("audioconvert").build()?;
        let audio_resample = gst::ElementFactory::make("audioresample").build()?;
        let audio_sink = gst::ElementFactory::make("autoaudiosink").build()?;
        
        pipeline.add_many([
            &video_concat, &audio_concat,
            &video_convert, &video_scale, preview_sink.upcast_ref::<gst::Element>(),
            &audio_convert, &audio_resample, &audio_sink,
        ])?;
        
        // Link output chains
        gst::Element::link_many([&video_concat, &video_convert, &video_scale, preview_sink.upcast_ref::<gst::Element>()])?;
        gst::Element::link_many([&audio_concat, &audio_convert, &audio_resample, &audio_sink])?;
        
        // Add decoders for each clip
        for (i, clip_path) in video_clips.iter().enumerate() {
            let uri = format!("file://{}", clip_path.canonicalize()?.display());
            
            let src = gst::ElementFactory::make("uridecodebin")
                .name(format!("src_{}", i))
                .property("uri", &uri)
                .build()?;
            
            pipeline.add(&src)?;
            
            let video_concat_weak = video_concat.downgrade();
            let audio_concat_weak = audio_concat.downgrade();
            
            src.connect_pad_added(move |_, pad| {
                let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
                let structure = caps.structure(0).unwrap();
                let name = structure.name();
                
                if name.starts_with("video/")
                    && let Some(concat) = video_concat_weak.upgrade()
                    && let Some(sink_pad) = concat.request_pad_simple("sink_%u")
                {
                    let _ = pad.link(&sink_pad);
                } else if name.starts_with("audio/")
                    && let Some(concat) = audio_concat_weak.upgrade()
                    && let Some(sink_pad) = concat.request_pad_simple("sink_%u")
                {
                    let _ = pad.link(&sink_pad);
                }
            });
        }
        
        // Set up frame callback
        let frame_ref = self.current_frame.clone();
        preview_sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;
                    
                    let video_info = gst_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;
                    
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    
                    let frame = Frame {
                        data: map.as_slice().to_vec(),
                        width: video_info.width(),
                        height: video_info.height(),
                    };
                    
                    *frame_ref.lock().unwrap() = Some(frame);
                    
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        
        Ok(())
    }
    
    /// Play
    pub fn play(&mut self) {
        if let Some(ref pipeline) = self.pipeline {
            let _ = pipeline.set_state(gst::State::Playing);
            self.state = PlayerState::Playing;
        }
    }
    
    /// Pause
    pub fn pause(&mut self) {
        if let Some(ref pipeline) = self.pipeline {
            let _ = pipeline.set_state(gst::State::Paused);
            self.state = PlayerState::Paused;
        }
    }
    
    /// Stop
    pub fn stop(&mut self) {
        if let Some(ref pipeline) = self.pipeline {
            let _ = pipeline.set_state(gst::State::Null);
        }
        self.pipeline = None;
        self.state = PlayerState::Stopped;
        *self.current_frame.lock().unwrap() = None;
    }
    
    /// Seek to position (0.0 to 1.0)
    pub fn seek(&self, position: f64) {
        if let Some(ref pipeline) = self.pipeline {
            let position_ns = (position.clamp(0.0, 1.0) * self.duration * 1_000_000_000.0) as u64;
            let _ = pipeline.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_nseconds(position_ns),
            );
        }
    }
    
    /// Get current position (0.0 to 1.0)
    pub fn get_position(&self) -> f64 {
        if let Some(ref pipeline) = self.pipeline
            && let Some(pos) = pipeline.query_position::<gst::ClockTime>()
            && self.duration > 0.0
        {
            return pos.nseconds() as f64 / (self.duration * 1_000_000_000.0);
        }
        0.0
    }
    
    /// Get current frame
    pub fn current_frame(&self) -> Option<Frame> {
        self.current_frame.lock().unwrap().clone()
    }
    
    /// Get state
    pub fn state(&self) -> PlayerState {
        self.state
    }
    
    /// Get duration
    pub fn duration(&self) -> f64 {
        self.duration
    }
    
    /// Check if loaded
    pub fn is_loaded(&self) -> bool {
        self.pipeline.is_some()
    }
}

impl Drop for ProjectPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}
