use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::Path;
use std::process::Command;

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
    // Get video clips
    let video_clips: Vec<&Clip> = project
        .clips
        .iter()
        .filter(|c| c.media_type == MediaType::Video)
        .collect();

    if video_clips.is_empty() {
        anyhow::bail!("No video clips to export");
    }

    // Get the main audio track (voiceover)
    let audio_track = project.audio.as_ref().map(|a| &a.path);

    tracing::info!(
        "Exporting {} video clips to {:?}, audio: {:?}",
        video_clips.len(),
        settings.output_path,
        audio_track
    );

    // Try FFmpeg first (most reliable for concat)
    if is_ffmpeg_available() {
        tracing::info!("Using FFmpeg for export");
        return export_with_ffmpeg(&video_clips, audio_track, settings);
    }

    // Fall back to GStreamer
    tracing::info!("Using GStreamer for export");
    
    if video_clips.len() == 1 {
        export_single_clip_gst(&video_clips[0].path, audio_track, settings, on_progress)
    } else {
        export_multiple_clips_gst(&video_clips, audio_track, settings, on_progress)
    }
}

/// Check if FFmpeg is available
fn is_ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Export using FFmpeg (more reliable for concatenation)
fn export_with_ffmpeg(
    video_clips: &[&Clip],
    audio_track: Option<&std::path::PathBuf>,
    settings: &ExportSettings,
) -> Result<()> {
    let temp_dir = std::env::temp_dir().join("montage_export");
    std::fs::create_dir_all(&temp_dir)?;

    // Create a concat file list
    let concat_file = temp_dir.join("concat.txt");
    let mut concat_content = String::new();
    
    for clip in video_clips {
        let path = clip.path.canonicalize()
            .unwrap_or_else(|_| clip.path.clone());
        // FFmpeg concat format: file 'path'
        concat_content.push_str(&format!("file '{}'\n", path.display()));
    }
    
    std::fs::write(&concat_file, &concat_content)?;
    tracing::debug!("Concat file:\n{}", concat_content);

    let output_path = settings.output_path.to_string_lossy();
    
    // Build FFmpeg command
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y"); // Overwrite output
    
    // Input: concatenated videos
    cmd.args(["-f", "concat", "-safe", "0", "-i"]);
    cmd.arg(&concat_file);
    
    // Input: audio track (if provided)
    if let Some(audio_path) = audio_track {
        cmd.args(["-i"]);
        cmd.arg(audio_path);
    }
    
    // Video settings
    cmd.args([
        "-c:v", "libx264",
        "-preset", "medium",
        "-b:v", &format!("{}k", settings.video_bitrate),
        "-vf", &format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
            settings.width, settings.height, settings.width, settings.height),
    ]);
    
    // Audio settings
    if audio_track.is_some() {
        // Use the separate audio track, not the video's audio
        cmd.args([
            "-map", "0:v:0",     // Video from concat
            "-map", "1:a:0",     // Audio from separate track
            "-c:a", "aac",
            "-b:a", &format!("{}k", settings.audio_bitrate),
            "-shortest",        // End when shortest stream ends
        ]);
    } else {
        // Use audio from videos
        cmd.args([
            "-c:a", "aac",
            "-b:a", &format!("{}k", settings.audio_bitrate),
        ]);
    }
    
    cmd.arg(&*output_path);
    
    tracing::info!("Running FFmpeg: {:?}", cmd);
    
    let output = cmd.output().context("Failed to run FFmpeg")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("FFmpeg stderr: {}", stderr);
        anyhow::bail!("FFmpeg failed: {}", stderr.lines().last().unwrap_or("unknown error"));
    }
    
    // Clean up
    let _ = std::fs::remove_file(&concat_file);
    
    tracing::info!("Export complete: {}", output_path);
    Ok(())
}

/// Export a single clip with optional audio overlay using GStreamer
fn export_single_clip_gst(
    video_path: &Path,
    audio_track: Option<&std::path::PathBuf>,
    settings: &ExportSettings,
    _on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let video_uri = format!("file://{}", video_path.canonicalize()?.display());
    let output_path = settings.output_path.to_string_lossy();

    let pipeline_str = if let Some(audio_path) = audio_track {
        let audio_uri = format!("file://{}", audio_path.canonicalize()?.display());
        format!(
            r#"
            uridecodebin uri="{}" name=vdec
            uridecodebin uri="{}" name=adec
            vdec. ! queue ! videoconvert ! videoscale ! 
                video/x-raw,width={},height={} ! 
                x264enc bitrate={} ! h264parse ! queue ! mux.
            adec. ! queue ! audioconvert ! audioresample ! 
                audio/x-raw,rate=48000,channels=2 !
                fdkaacenc bitrate={} ! queue ! mux.
            mp4mux name=mux ! filesink location="{}"
            "#,
            video_uri,
            audio_uri,
            settings.width,
            settings.height,
            settings.video_bitrate,
            settings.audio_bitrate * 1000,
            output_path
        )
    } else {
        format!(
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
            video_uri,
            settings.width,
            settings.height,
            settings.video_bitrate,
            settings.audio_bitrate * 1000,
            output_path
        )
    };

    run_gst_pipeline(&pipeline_str)
}

/// Export multiple clips using GStreamer (fallback)
fn export_multiple_clips_gst(
    clips: &[&Clip],
    audio_track: Option<&std::path::PathBuf>,
    settings: &ExportSettings,
    _on_progress: Option<ProgressCallback>,
) -> Result<()> {
    // For GStreamer, we'll use splitmuxsink approach or manual concat
    // This is complex and error-prone, so we really want FFmpeg
    
    tracing::warn!("GStreamer multi-clip export is experimental. Install FFmpeg for better results.");
    
    // Create a temporary script to concat with GStreamer
    // For now, just export the first clip as a fallback
    if clips.is_empty() {
        anyhow::bail!("No clips to export");
    }
    
    tracing::warn!("Exporting only first clip (install FFmpeg for full concat support)");
    export_single_clip_gst(&clips[0].path, audio_track, settings, None)
}

/// Run a GStreamer pipeline from string
fn run_gst_pipeline(pipeline_str: &str) -> Result<()> {
    tracing::debug!("GStreamer pipeline:\n{}", pipeline_str);
    
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
                tracing::info!("GStreamer: End of stream");
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                let debug_str = err.debug()
                    .map(|d| format!("{:?}", d))
                    .unwrap_or_default();
                tracing::error!("GStreamer error: {} ({})", err.error(), debug_str);
                anyhow::bail!("GStreamer error: {}", err.error());
            }
            MessageView::Warning(warn) => {
                tracing::warn!("GStreamer warning: {}", warn.error());
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
    }
    
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
