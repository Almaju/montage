use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::Path;

use crate::project::{Clip, MediaType, Project};

/// Export settings
#[derive(Clone, Debug)]
pub struct ExportSettings {
    /// Output file path
    pub output_path: std::path::PathBuf,
    /// Video width (default: 1920)
    pub width: u32,
    /// Video height (default: 1080)
    pub height: u32,
    /// Video bitrate in kbps (default: 5000)
    pub video_bitrate: u32,
    /// Audio bitrate in kbps (default: 192)
    pub audio_bitrate: u32,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            output_path: std::path::PathBuf::from("output.mp4"),
            width: 1920,
            height: 1080,
            video_bitrate: 5000,
            audio_bitrate: 192,
        }
    }
}

/// Export progress callback
pub type ProgressCallback = Box<dyn Fn(f64) + Send>;

/// Export a project to a video file
pub fn export_project(
    project: &Project,
    settings: &ExportSettings,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    // Get video clips only for now
    let video_clips: Vec<&Clip> = project
        .clips
        .iter()
        .filter(|c| c.media_type == MediaType::Video)
        .collect();

    if video_clips.is_empty() {
        anyhow::bail!("No video clips to export");
    }

    tracing::info!("Exporting {} video clips to {:?}", video_clips.len(), settings.output_path);

    // For single clip, use simple pipeline
    if video_clips.len() == 1 {
        return export_single_clip(&video_clips[0].path, settings, on_progress);
    }

    // For multiple clips, use concat
    export_multiple_clips(&video_clips, settings, on_progress)
}

/// Export a single clip (simple re-encode)
fn export_single_clip(
    input_path: &Path,
    settings: &ExportSettings,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let input_uri = format!("file://{}", input_path.canonicalize()?.display());
    let output_path = settings.output_path.to_string_lossy();

    let pipeline_str = format!(
        r#"
        uridecodebin uri="{}" name=demux
        demux. ! queue ! videoconvert ! videoscale ! 
            video/x-raw,width={},height={} ! 
            x264enc bitrate={} ! h264parse ! queue ! mux.
        demux. ! queue ! audioconvert ! audioresample ! 
            audio/x-raw,rate=48000,channels=2 !
            fdkaacenc bitrate={} ! queue ! mux.
        mp4mux name=mux ! filesink location="{}"
        "#,
        input_uri,
        settings.width,
        settings.height,
        settings.video_bitrate,
        settings.audio_bitrate * 1000,
        output_path
    );

    run_pipeline(&pipeline_str, on_progress)
}

/// Export multiple clips by concatenating them
fn export_multiple_clips(
    clips: &[&Clip],
    settings: &ExportSettings,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    // Build a concat pipeline
    // This is more complex - we need to use GStreamer's concat element
    
    let pipeline = gst::Pipeline::new();
    
    // Create concat elements for video and audio
    let video_concat = gst::ElementFactory::make("concat")
        .name("video_concat")
        .build()
        .context("Failed to create video concat")?;
    
    let audio_concat = gst::ElementFactory::make("concat")
        .name("audio_concat")
        .build()
        .context("Failed to create audio concat")?;
    
    // Create output elements
    let video_convert = gst::ElementFactory::make("videoconvert").build()?;
    let video_scale = gst::ElementFactory::make("videoscale").build()?;
    let video_capsfilter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("width", settings.width as i32)
                .field("height", settings.height as i32)
                .build(),
        )
        .build()?;
    let video_encoder = gst::ElementFactory::make("x264enc")
        .property("bitrate", settings.video_bitrate)
        .build()?;
    let video_parser = gst::ElementFactory::make("h264parse").build()?;
    let video_queue = gst::ElementFactory::make("queue").build()?;
    
    let audio_convert = gst::ElementFactory::make("audioconvert").build()?;
    let audio_resample = gst::ElementFactory::make("audioresample").build()?;
    let audio_capsfilter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("rate", 48000i32)
                .field("channels", 2i32)
                .build(),
        )
        .build()?;
    let audio_encoder = gst::ElementFactory::make("fdkaacenc")
        .property("bitrate", (settings.audio_bitrate * 1000) as i32)
        .build()
        .or_else(|_| {
            // Fallback to voaacenc if fdkaacenc not available
            gst::ElementFactory::make("voaacenc")
                .property("bitrate", (settings.audio_bitrate * 1000) as i32)
                .build()
        })
        .context("No AAC encoder available")?;
    let audio_queue = gst::ElementFactory::make("queue").build()?;
    
    let muxer = gst::ElementFactory::make("mp4mux").build()?;
    let filesink = gst::ElementFactory::make("filesink")
        .property("location", settings.output_path.to_string_lossy().to_string())
        .build()?;
    
    // Add all elements to pipeline
    pipeline.add_many([
        &video_concat, &audio_concat,
        &video_convert, &video_scale, &video_capsfilter, &video_encoder, &video_parser, &video_queue,
        &audio_convert, &audio_resample, &audio_capsfilter, &audio_encoder, &audio_queue,
        &muxer, &filesink,
    ])?;
    
    // Link output chain
    gst::Element::link_many([
        &video_concat, &video_convert, &video_scale, &video_capsfilter, 
        &video_encoder, &video_parser, &video_queue,
    ])?;
    video_queue.link_pads(Some("src"), &muxer, Some("video_%u"))?;
    
    gst::Element::link_many([
        &audio_concat, &audio_convert, &audio_resample, &audio_capsfilter,
        &audio_encoder, &audio_queue,
    ])?;
    audio_queue.link_pads(Some("src"), &muxer, Some("audio_%u"))?;
    
    muxer.link(&filesink)?;
    
    // Add decoders for each clip
    for (i, clip) in clips.iter().enumerate() {
        let uri = format!("file://{}", clip.path.canonicalize()?.display());
        
        let decodebin = gst::ElementFactory::make("uridecodebin")
            .name(format!("decoder_{}", i))
            .property("uri", &uri)
            .build()?;
        
        pipeline.add(&decodebin)?;
        
        // Connect pad-added signal to link to concat
        let video_concat_weak = video_concat.downgrade();
        let audio_concat_weak = audio_concat.downgrade();
        
        decodebin.connect_pad_added(move |_element, pad| {
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            let structure = caps.structure(0).unwrap();
            let name = structure.name();
            
            if name.starts_with("video/")
                && let Some(concat) = video_concat_weak.upgrade()
            {
                let sink_pad = concat.request_pad_simple("sink_%u").unwrap();
                if pad.link(&sink_pad).is_err() {
                    tracing::warn!("Failed to link video pad");
                }
            } else if name.starts_with("audio/")
                && let Some(concat) = audio_concat_weak.upgrade()
            {
                let sink_pad = concat.request_pad_simple("sink_%u").unwrap();
                if pad.link(&sink_pad).is_err() {
                    tracing::warn!("Failed to link audio pad");
                }
            }
        });
    }
    
    // Run the pipeline
    pipeline.set_state(gst::State::Playing)?;
    
    let bus = pipeline.bus().unwrap();
    
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        
        match msg.view() {
            MessageView::Eos(..) => {
                tracing::info!("Export complete");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                anyhow::bail!(
                    "Export error: {} ({})",
                    err.error(),
                    err.debug().unwrap_or_default()
                );
            }
            MessageView::StateChanged(state) => {
                if state.src().map(|s| s == &pipeline).unwrap_or(false) {
                    tracing::debug!(
                        "Pipeline state: {:?} -> {:?}",
                        state.old(),
                        state.current()
                    );
                }
            }
            _ => {}
        }
        
        // Report progress
        if let Some(ref callback) = on_progress
            && let Some(duration) = pipeline.query_duration::<gst::ClockTime>()
            && let Some(position) = pipeline.query_position::<gst::ClockTime>()
        {
            let progress = position.nseconds() as f64 / duration.nseconds() as f64;
            callback(progress);
        }
    }
    
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

/// Run a pipeline from a string description
fn run_pipeline(pipeline_str: &str, on_progress: Option<ProgressCallback>) -> Result<()> {
    let pipeline = gst::parse::launch(pipeline_str)
        .context("Failed to create pipeline")?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow::anyhow!("Not a pipeline"))?;
    
    pipeline.set_state(gst::State::Playing)?;
    
    let bus = pipeline.bus().unwrap();
    
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;
        
        match msg.view() {
            MessageView::Eos(..) => {
                tracing::info!("Export complete");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                anyhow::bail!(
                    "Export error: {} ({})",
                    err.error(),
                    err.debug().unwrap_or_default()
                );
            }
            _ => {}
        }
        
        // Report progress
        if let Some(ref callback) = on_progress
            && let Some(duration) = pipeline.query_duration::<gst::ClockTime>()
            && let Some(position) = pipeline.query_position::<gst::ClockTime>()
        {
            let progress = position.nseconds() as f64 / duration.nseconds() as f64;
            callback(progress);
        }
    }
    
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
