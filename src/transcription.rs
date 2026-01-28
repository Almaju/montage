use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// A segment of transcribed audio with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Start time in seconds
    pub start: f64,
    /// End time in seconds
    pub end: f64,
    /// Transcribed text
    pub text: String,
}

/// Full transcript with segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    /// Full text
    pub text: String,
    /// Individual segments with timing
    pub segments: Vec<TranscriptSegment>,
    /// Language detected
    pub language: Option<String>,
    /// Duration in seconds
    pub duration: f64,
}

/// Transcribe an audio file using Whisper
/// 
/// Tries multiple methods:
/// 1. whisper-cpp CLI if installed
/// 2. Ollama with whisper model (if available)
/// 3. Python whisper as fallback
pub fn transcribe(audio_path: &Path) -> Result<Transcript> {
    // Try whisper.cpp first (fastest)
    if let Ok(transcript) = transcribe_with_whisper_cpp(audio_path) {
        return Ok(transcript);
    }
    
    // Try insanely-fast-whisper or whisper CLI
    if let Ok(transcript) = transcribe_with_whisper_cli(audio_path) {
        return Ok(transcript);
    }
    
    anyhow::bail!(
        "No whisper installation found. Please install one of:\n\
         - whisper.cpp: https://github.com/ggerganov/whisper.cpp\n\
         - whisper: pip install openai-whisper\n\
         - insanely-fast-whisper: pip install insanely-fast-whisper"
    )
}

/// Transcribe using whisper.cpp CLI
fn transcribe_with_whisper_cpp(audio_path: &Path) -> Result<Transcript> {
    // whisper.cpp outputs JSON with -oj flag
    let output = Command::new("whisper-cpp")
        .args([
            "-m", "base.en",  // or path to model
            "-f", &audio_path.to_string_lossy(),
            "-oj",  // output JSON
            "--print-progress", "false",
        ])
        .output();
    
    // Also try "main" binary name (common whisper.cpp build name)
    let output = output.or_else(|_| {
        Command::new("main")
            .args([
                "-m", "/usr/local/share/whisper/ggml-base.en.bin",
                "-f", &audio_path.to_string_lossy(),
                "-oj",
            ])
            .output()
    })?;
    
    if !output.status.success() {
        anyhow::bail!("whisper.cpp failed");
    }
    
    // Parse JSON output
    let json_str = String::from_utf8(output.stdout)?;
    parse_whisper_json(&json_str)
}

/// Transcribe using Python whisper CLI
fn transcribe_with_whisper_cli(audio_path: &Path) -> Result<Transcript> {
    // Create temp dir for output
    let temp_dir = std::env::temp_dir().join("montage_whisper");
    std::fs::create_dir_all(&temp_dir)?;
    
    let output = Command::new("whisper")
        .args([
            &audio_path.to_string_lossy(),
            "--model", "base",
            "--output_format", "json",
            "--output_dir", &temp_dir.to_string_lossy(),
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("whisper failed: {}", stderr);
    }
    
    // Find the JSON output file
    let audio_stem = audio_path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "audio".to_string());
    
    let json_path = temp_dir.join(format!("{}.json", audio_stem));
    let json_str = std::fs::read_to_string(&json_path)
        .context("Failed to read whisper output")?;
    
    // Clean up
    let _ = std::fs::remove_file(&json_path);
    
    parse_whisper_json(&json_str)
}

/// Parse Whisper JSON output
fn parse_whisper_json(json_str: &str) -> Result<Transcript> {
    #[derive(Deserialize)]
    struct WhisperOutput {
        text: String,
        segments: Vec<WhisperSegment>,
        language: Option<String>,
    }
    
    #[derive(Deserialize)]
    struct WhisperSegment {
        start: f64,
        end: f64,
        text: String,
    }
    
    let output: WhisperOutput = serde_json::from_str(json_str)
        .context("Failed to parse whisper JSON")?;
    
    let duration = output.segments.last()
        .map(|s| s.end)
        .unwrap_or(0.0);
    
    Ok(Transcript {
        text: output.text.trim().to_string(),
        segments: output.segments.into_iter().map(|s| TranscriptSegment {
            start: s.start,
            end: s.end,
            text: s.text.trim().to_string(),
        }).collect(),
        language: output.language,
        duration,
    })
}

/// Check if whisper is available
#[allow(dead_code)]
pub fn is_available() -> bool {
    Command::new("whisper")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    || Command::new("whisper-cpp")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
