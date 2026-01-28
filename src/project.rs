use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Montage project file format
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Project {
    /// Project format version
    pub version: u32,
    
    /// Project metadata
    pub metadata: ProjectMetadata,
    
    /// Audio track configuration (main audio)
    pub audio: Option<AudioTrack>,
    
    /// Video track configuration (legacy, for single video)
    pub video: Option<VideoTrack>,
    
    /// Media clips in the project
    #[serde(default)]
    pub clips: Vec<Clip>,
    
    /// Timeline state
    pub timeline: TimelineState,
}

/// A media clip with description and timing
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Clip {
    /// Unique clip ID
    pub id: String,
    
    /// User description (e.g., "introduction part")
    pub description: String,
    
    /// Path to the media file
    pub path: PathBuf,
    
    /// Type of media
    pub media_type: MediaType,
    
    /// Start time in the timeline (seconds)
    #[serde(default)]
    pub start_time: f64,
    
    /// Duration of the clip (seconds)
    #[serde(default)]
    pub duration: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Audio,
    Video,
    Image,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProjectMetadata {
    /// Project name
    pub name: String,
    
    /// Project description
    #[serde(default)]
    pub description: String,
    
    /// Creation timestamp (ISO 8601)
    #[serde(default)]
    pub created_at: Option<String>,
    
    /// Last modified timestamp (ISO 8601)
    #[serde(default)]
    pub modified_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AudioTrack {
    /// Path to the audio file
    pub path: PathBuf,
    
    /// Audio duration in seconds (cached)
    #[serde(default)]
    pub duration: Option<f64>,
    
    /// Sample rate (cached)
    #[serde(default)]
    pub sample_rate: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VideoTrack {
    /// Path to the video file
    pub path: PathBuf,
    
    /// Video duration in seconds (cached)
    #[serde(default)]
    pub duration: Option<f64>,
    
    /// Video dimensions (cached)
    #[serde(default)]
    pub dimensions: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TimelineState {
    /// Current playhead position in seconds
    #[serde(default)]
    pub position: f64,
    
    /// Zoom level (pixels per second)
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    10.0
}

impl Project {
    /// Current project format version
    pub const CURRENT_VERSION: u32 = 1;
    
    /// File extension for project files
    pub const EXTENSION: &'static str = "montage";
    
    /// Create a new empty project
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono_now();
        Self {
            version: Self::CURRENT_VERSION,
            metadata: ProjectMetadata {
                name: name.into(),
                description: String::new(),
                created_at: Some(now.clone()),
                modified_at: Some(now),
            },
            audio: None,
            video: None,
            clips: Vec::new(),
            timeline: TimelineState::default(),
        }
    }
    
    /// Load a project from a file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .context("Failed to read project file")?;
        
        let project: Self = serde_json::from_str(&content)
            .context("Failed to parse project file")?;
        
        Ok(project)
    }
    
    /// Save the project to a file
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.metadata.modified_at = Some(chrono_now());
        
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize project")?;
        
        std::fs::write(path, content)
            .context("Failed to write project file")?;
        
        Ok(())
    }
    
    /// Set the audio track
    pub fn set_audio(&mut self, path: PathBuf, duration: f64, sample_rate: u32) {
        self.audio = Some(AudioTrack {
            path,
            duration: Some(duration),
            sample_rate: Some(sample_rate),
        });
    }
    
    /// Set the video track
    pub fn set_video(&mut self, path: PathBuf, duration: f64, dimensions: (u32, u32)) {
        self.video = Some(VideoTrack {
            path,
            duration: Some(duration),
            dimensions: Some(dimensions),
        });
    }
    
    /// Add a clip to the project
    pub fn add_clip(&mut self, description: String, path: PathBuf) -> &Clip {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Generate a simple unique ID
        let id = format!(
            "clip_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        
        // Detect media type from extension
        let media_type = match path.extension().and_then(|e| e.to_str()) {
            Some("mp3") | Some("wav") | Some("flac") | Some("ogg") | Some("m4a") => MediaType::Audio,
            Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") => MediaType::Image,
            _ => MediaType::Video, // Default to video
        };
        
        // Calculate start time (end of last clip)
        let start_time = self.clips
            .last()
            .map(|c| c.start_time + c.duration.unwrap_or(0.0))
            .unwrap_or(0.0);
        
        self.clips.push(Clip {
            id,
            description,
            path,
            media_type,
            start_time,
            duration: None, // Will be filled when media is loaded
        });
        
        self.clips.last().unwrap()
    }
    
    /// Get all video clips
    #[allow(dead_code)]
    pub fn video_clips(&self) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(|c| c.media_type == MediaType::Video)
    }
    
    /// Get all audio clips
    #[allow(dead_code)]
    pub fn audio_clips(&self) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(|c| c.media_type == MediaType::Audio)
    }
}

/// Get current timestamp in ISO 8601 format
fn chrono_now() -> String {
    // Simple timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_project_roundtrip() {
        let mut project = Project::new("Test Project");
        project.set_audio(
            PathBuf::from("/path/to/audio.mp3"),
            120.5,
            44100,
        );
        project.timeline.position = 30.0;
        
        let json = serde_json::to_string_pretty(&project).unwrap();
        let loaded: Project = serde_json::from_str(&json).unwrap();
        
        assert_eq!(loaded.metadata.name, "Test Project");
        assert_eq!(loaded.timeline.position, 30.0);
        assert!(loaded.audio.is_some());
    }
}
