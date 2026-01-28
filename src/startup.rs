/// Status of required services
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub ollama: OllamaStatus,
    pub whisper: WhisperStatus,
    pub pexels: PexelsStatus,
}

#[derive(Debug, Clone)]
pub enum OllamaStatus {
    Ready(String), // model name
    NotRunning,
    NoModel,
}

#[derive(Debug, Clone)]
pub enum WhisperStatus {
    Available(String), // which whisper
    NotInstalled,
}

#[derive(Debug, Clone)]
pub enum PexelsStatus {
    Configured,
    NotConfigured,
}

impl ServiceStatus {
    /// Check all services
    pub fn check(pexels_key: &Option<String>) -> Self {
        Self {
            ollama: check_ollama(),
            whisper: check_whisper(),
            pexels: if pexels_key.as_ref().is_some_and(|k| !k.is_empty()) {
                PexelsStatus::Configured
            } else {
                PexelsStatus::NotConfigured
            },
        }
    }
    
    /// Generate a greeting message based on status
    pub fn greeting_message(&self) -> String {
        let mut lines = vec![
            "👋 **Welcome to Montage!**".to_string(),
            String::new(),
            "I'm your AI video editing assistant. Here's the current status:".to_string(),
            String::new(),
        ];
        
        // Ollama status
        match &self.ollama {
            OllamaStatus::Ready(model) => {
                lines.push(format!("✅ **Ollama**: Ready ({})", model));
            }
            OllamaStatus::NotRunning => {
                lines.push("❌ **Ollama**: Not running".to_string());
                lines.push("   → Run `ollama serve` in a terminal".to_string());
            }
            OllamaStatus::NoModel => {
                lines.push("⚠️ **Ollama**: Running but no model".to_string());
                lines.push("   → Run `ollama pull qwen2.5:3b`".to_string());
            }
        }
        
        // Whisper status
        match &self.whisper {
            WhisperStatus::Available(which) => {
                lines.push(format!("✅ **Whisper**: Available ({})", which));
            }
            WhisperStatus::NotInstalled => {
                lines.push("⚠️ **Whisper**: Not installed (optional)".to_string());
                lines.push("   → `pip install openai-whisper` for audio transcription".to_string());
            }
        }
        
        // Pexels status
        match &self.pexels {
            PexelsStatus::Configured => {
                lines.push("✅ **Pexels API**: Configured".to_string());
            }
            PexelsStatus::NotConfigured => {
                lines.push("⚠️ **Pexels API**: Not configured (optional)".to_string());
                lines.push("   → Say: \"set pexels key YOUR_API_KEY\"".to_string());
                lines.push("   → Get free key at: pexels.com/api".to_string());
            }
        }
        
        lines.push(String::new());
        
        // Ready state
        if matches!(self.ollama, OllamaStatus::Ready(_)) {
            lines.push("🎬 **Ready to edit!** Drag & drop videos or type a command.".to_string());
        } else {
            lines.push("⏳ **Setup needed**: Please start Ollama to use AI features.".to_string());
        }
        
        lines.join("\n")
    }
    
    /// Get quick status indicators for the UI
    pub fn status_indicators(&self) -> Vec<(String, bool)> {
        vec![
            ("Ollama".to_string(), matches!(self.ollama, OllamaStatus::Ready(_))),
            ("Whisper".to_string(), matches!(self.whisper, WhisperStatus::Available(_))),
            ("Pexels".to_string(), matches!(self.pexels, PexelsStatus::Configured)),
        ]
    }
}

/// Check if Ollama is running and has the model
fn check_ollama() -> OllamaStatus {
    let client = reqwest::blocking::Client::new();
    
    // Check if Ollama is running
    let response = client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(2))
        .send();
    
    match response {
        Ok(resp) if resp.status().is_success() => {
            // Check if our model is available
            if let Ok(body) = resp.text() {
                if body.contains("qwen2.5") {
                    return OllamaStatus::Ready("qwen2.5:3b".to_string());
                } else if body.contains("llama") {
                    return OllamaStatus::Ready("llama".to_string());
                }
            }
            OllamaStatus::NoModel
        }
        _ => OllamaStatus::NotRunning,
    }
}

/// Check if Whisper is installed
fn check_whisper() -> WhisperStatus {
    use std::process::Command;
    
    // Try whisper.cpp
    if Command::new("whisper-cpp")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return WhisperStatus::Available("whisper.cpp".to_string());
    }
    
    // Try main (whisper.cpp alternate name)
    if Command::new("main")
        .arg("--help")
        .output()
        .map(|o| !o.stderr.is_empty() || !o.stdout.is_empty())
        .unwrap_or(false)
    {
        return WhisperStatus::Available("whisper.cpp".to_string());
    }
    
    // Try Python whisper
    if Command::new("whisper")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return WhisperStatus::Available("openai-whisper".to_string());
    }
    
    WhisperStatus::NotInstalled
}
