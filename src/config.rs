use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// App configuration stored between sessions
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    /// Path to the last opened project
    pub last_project: Option<PathBuf>,
    
    /// Recent projects (most recent first)
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    
    /// Pexels API key for stock footage
    #[serde(default)]
    pub pexels_api_key: Option<String>,
}

impl AppConfig {
    /// Maximum number of recent projects to remember
    const MAX_RECENT: usize = 10;
    
    /// Get the config file path (~/.montage/config.json)
    fn config_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .context("Could not find home directory")?;
        
        let montage_dir = home_dir.join(".montage");
        std::fs::create_dir_all(&montage_dir)?;
        
        Ok(montage_dir.join("config.json"))
    }
    
    /// Load config from disk, or return default
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }
    
    fn try_load() -> Result<Self> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }
    
    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    /// Record that a project was opened
    pub fn set_last_project(&mut self, path: PathBuf) {
        // Remove from recent if already there
        self.recent_projects.retain(|p| p != &path);
        
        // Add to front of recent
        self.recent_projects.insert(0, path.clone());
        
        // Trim to max
        self.recent_projects.truncate(Self::MAX_RECENT);
        
        // Set as last
        self.last_project = Some(path);
        
        // Save immediately
        if let Err(e) = self.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
    }
    
    /// Set the Pexels API key
    pub fn set_pexels_api_key(&mut self, key: String) {
        self.pexels_api_key = Some(key);
        if let Err(e) = self.save() {
            tracing::warn!("Failed to save config: {}", e);
        }
    }
    
    /// Check if Pexels API key is configured
    #[allow(dead_code)]
    pub fn has_pexels_key(&self) -> bool {
        self.pexels_api_key.as_ref().is_some_and(|k| !k.is_empty())
    }
}
