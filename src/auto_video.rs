use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::pexels::{self, PexelsVideo};
use crate::transcription::{self, Transcript, TranscriptSegment};

/// A suggested video clip based on transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedClip {
    /// Search query used
    pub query: String,
    /// Segment of the transcript this covers
    pub segment: TranscriptSegment,
    /// Matched video from Pexels
    pub video: Option<PexelsVideo>,
    /// Local path if downloaded
    pub local_path: Option<PathBuf>,
}

/// Result of auto-video generation
#[derive(Debug, Clone)]
pub struct AutoVideoResult {
    /// The transcript
    pub transcript: Transcript,
    /// Suggested clips for each segment
    pub clips: Vec<SuggestedClip>,
}

/// Extract keywords from transcript segments for video search
/// Uses the LLM to analyze the transcript and suggest search queries
pub fn extract_keywords_with_llm(
    transcript: &Transcript,
    ollama_available: bool,
) -> Result<Vec<(TranscriptSegment, String)>> {
    if ollama_available {
        extract_keywords_ollama(transcript)
    } else {
        // Fallback: simple keyword extraction
        Ok(extract_keywords_simple(transcript))
    }
}

/// Use Ollama to extract meaningful search queries
fn extract_keywords_ollama(transcript: &Transcript) -> Result<Vec<(TranscriptSegment, String)>> {
    
    let segments_json = serde_json::to_string_pretty(&transcript.segments)?;
    
    let prompt = format!(
        r#"Analyze these transcript segments and suggest a Pexels video search query for each.
Return JSON array with "segment_index" and "query" for each.

Segments:
{}

Rules:
- Query should be 1-3 words, visual/concrete (e.g., "sunset beach", "city traffic", "typing keyboard")
- Match the mood and topic of the speech
- Avoid abstract concepts, focus on filmable subjects

Return ONLY valid JSON array like:
[{{"segment_index": 0, "query": "nature landscape"}}, ...]"#,
        segments_json
    );
    
    #[derive(Deserialize)]
    struct QuerySuggestion {
        segment_index: usize,
        query: String,
    }
    
    // Call Ollama directly with a simpler request
    let request = serde_json::json!({
        "model": "qwen2.5:3b",
        "prompt": prompt,
        "stream": false,
        "format": "json"
    });
    
    let client = reqwest::blocking::Client::new();
    let response = client
        .post("http://localhost:11434/api/generate")
        .json(&request)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .context("Failed to connect to Ollama")?;
    
    if !response.status().is_success() {
        anyhow::bail!("Ollama error: {}", response.status());
    }
    
    #[derive(Deserialize)]
    struct OllamaResponse {
        response: String,
    }
    
    let ollama_resp: OllamaResponse = response.json()?;
    let suggestions: Vec<QuerySuggestion> = serde_json::from_str(&ollama_resp.response)
        .context("Failed to parse LLM suggestions")?;
    
    let results: Vec<_> = suggestions.into_iter()
        .filter_map(|s| {
            transcript.segments.get(s.segment_index)
                .map(|seg| (seg.clone(), s.query))
        })
        .collect();
    
    Ok(results)
}

/// Simple keyword extraction without LLM
fn extract_keywords_simple(transcript: &Transcript) -> Vec<(TranscriptSegment, String)> {
    // Common filler words to ignore
    let stopwords: std::collections::HashSet<&str> = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
        "of", "with", "by", "from", "is", "are", "was", "were", "be", "been",
        "being", "have", "has", "had", "do", "does", "did", "will", "would",
        "could", "should", "may", "might", "must", "shall", "can", "need",
        "this", "that", "these", "those", "i", "you", "he", "she", "it", "we",
        "they", "what", "which", "who", "whom", "whose", "where", "when", "why",
        "how", "all", "each", "every", "both", "few", "more", "most", "other",
        "some", "such", "no", "nor", "not", "only", "own", "same", "so", "than",
        "too", "very", "just", "also", "now", "here", "there", "then", "once",
    ].into_iter().collect();
    
    transcript.segments.iter().map(|segment| {
        // Extract meaningful words
        let lowercase_text = segment.text.to_lowercase();
        let words: Vec<&str> = lowercase_text
            .split_whitespace()
            .filter(|w| w.len() > 3 && !stopwords.contains(*w))
            .take(2)
            .collect();
        
        let query = if words.is_empty() {
            "abstract background".to_string()
        } else {
            words.join(" ")
        };
        
        (segment.clone(), query)
    }).collect()
}

/// Generate video suggestions from audio
pub fn generate_from_audio(
    audio_path: &Path,
    pexels_api_key: &str,
    output_dir: &Path,
) -> Result<AutoVideoResult> {
    // Step 1: Transcribe audio
    tracing::info!("Transcribing audio: {:?}", audio_path);
    let transcript = transcription::transcribe(audio_path)
        .context("Failed to transcribe audio")?;
    
    tracing::info!("Transcript: {} segments, {:.1}s duration", 
        transcript.segments.len(), transcript.duration);
    
    // Step 2: Extract keywords for each segment
    tracing::info!("Extracting keywords...");
    let keywords = extract_keywords_with_llm(&transcript, true)
        .unwrap_or_else(|e| {
            tracing::warn!("LLM keyword extraction failed: {}, using simple extraction", e);
            extract_keywords_simple(&transcript)
        });
    
    // Step 3: Search Pexels for each keyword
    tracing::info!("Searching Pexels for {} segments...", keywords.len());
    std::fs::create_dir_all(output_dir)?;
    
    let mut clips = Vec::new();
    for (segment, query) in keywords {
        tracing::info!("Searching for: '{}'", query);
        
        let video = match pexels::search_videos(pexels_api_key, &query, 3) {
            Ok(videos) => {
                // Pick a video that's long enough for the segment
                let segment_duration = (segment.end - segment.start) as u32;
                videos.into_iter()
                    .find(|v| v.duration >= segment_duration.max(3))
            }
            Err(e) => {
                tracing::warn!("Pexels search failed for '{}': {}", query, e);
                None
            }
        };
        
        clips.push(SuggestedClip {
            query,
            segment,
            video,
            local_path: None,
        });
    }
    
    Ok(AutoVideoResult { transcript, clips })
}

/// Download all suggested videos
pub fn download_clips(
    result: &mut AutoVideoResult,
    output_dir: &Path,
    _pexels_api_key: &str,
) -> Result<()> {
    for (i, clip) in result.clips.iter_mut().enumerate() {
        if let Some(ref video) = clip.video {
            let filename = format!("clip_{:03}_{}.mp4", i, clip.query.replace(' ', "_"));
            let output_path = output_dir.join(&filename);
            
            if !output_path.exists() {
                tracing::info!("Downloading clip {}: {}", i, clip.query);
                if let Err(e) = pexels::download_video(video, &output_path) {
                    tracing::warn!("Failed to download clip {}: {}", i, e);
                    continue;
                }
            }
            
            clip.local_path = Some(output_path);
        }
    }
    
    Ok(())
}
