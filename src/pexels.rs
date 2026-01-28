use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const PEXELS_API_URL: &str = "https://api.pexels.com/videos/search";

/// A video from Pexels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PexelsVideo {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub duration: u32,
    pub url: String,
    /// Direct download URL for the best quality
    pub video_url: String,
    /// Thumbnail/preview image
    pub image: String,
    /// User who uploaded
    pub user: String,
}

#[derive(Debug, Deserialize)]
struct PexelsResponse {
    videos: Vec<PexelsVideoRaw>,
}

#[derive(Debug, Deserialize)]
struct PexelsVideoRaw {
    id: u64,
    width: u32,
    height: u32,
    duration: u32,
    url: String,
    image: String,
    user: PexelsUser,
    video_files: Vec<PexelsVideoFile>,
}

#[derive(Debug, Deserialize)]
struct PexelsUser {
    name: String,
}

#[derive(Debug, Deserialize)]
struct PexelsVideoFile {
    link: String,
    quality: String,
    width: u32,
    height: u32,
}

/// Search for videos on Pexels
pub fn search_videos(api_key: &str, query: &str, per_page: u32) -> Result<Vec<PexelsVideo>> {
    let client = reqwest::blocking::Client::new();
    
    let response = client
        .get(PEXELS_API_URL)
        .header("Authorization", api_key)
        .query(&[
            ("query", query),
            ("per_page", &per_page.to_string()),
            ("orientation", "landscape"),
        ])
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .context("Failed to connect to Pexels API")?;
    
    if !response.status().is_success() {
        if response.status().as_u16() == 401 {
            anyhow::bail!("Invalid Pexels API key");
        }
        anyhow::bail!("Pexels API error: {}", response.status());
    }
    
    let pexels_response: PexelsResponse = response.json()
        .context("Failed to parse Pexels response")?;
    
    let videos = pexels_response.videos.into_iter().map(|v| {
        // Find the best quality video file (prefer HD)
        let video_url = v.video_files.iter()
            .filter(|f| f.quality == "hd" || f.quality == "sd")
            .max_by_key(|f| f.width * f.height)
            .map(|f| f.link.clone())
            .unwrap_or_default();
        
        PexelsVideo {
            id: v.id,
            width: v.width,
            height: v.height,
            duration: v.duration,
            url: v.url,
            video_url,
            image: v.image,
            user: v.user.name,
        }
    }).collect();
    
    Ok(videos)
}

/// Download a video to a local file
pub fn download_video(video: &PexelsVideo, output_path: &std::path::Path) -> Result<()> {
    let client = reqwest::blocking::Client::new();
    
    tracing::info!("Downloading video from Pexels: {}", video.video_url);
    
    let response = client
        .get(&video.video_url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .context("Failed to download video")?;
    
    if !response.status().is_success() {
        anyhow::bail!("Download failed: {}", response.status());
    }
    
    let bytes = response.bytes()?;
    std::fs::write(output_path, &bytes)?;
    
    tracing::info!("Downloaded {} bytes to {:?}", bytes.len(), output_path);
    
    Ok(())
}

/// Validate an API key by making a test request
#[allow(dead_code)]
pub fn validate_api_key(api_key: &str) -> bool {
    search_videos(api_key, "nature", 1).is_ok()
}
