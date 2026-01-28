use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const MODEL: &str = "llama3.2";

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    format: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Parsed action from the LLM
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ParsedAction {
    /// Add media with a description
    AddClip {
        description: String,
    },
    /// Set the project name
    SetProjectName {
        name: String,
    },
    /// Mark a time range (e.g., "from 0:30 to 1:00 is the intro")
    MarkRange {
        description: String,
        #[serde(default)]
        start_seconds: Option<f64>,
        #[serde(default)]
        end_seconds: Option<f64>,
    },
    /// Cut/trim at a specific time
    CutAt {
        seconds: f64,
    },
    /// Delete a clip by description
    DeleteClip {
        description: String,
    },
    /// Unknown/unclear command
    Unknown {
        message: String,
    },
}

const SYSTEM_PROMPT: &str = r#"You are a video editing assistant. Parse the user's command and return a JSON object with the action to take.

Available actions:
- add_clip: User wants to add media. Extract the description.
  {"action": "add_clip", "description": "intro section"}

- set_project_name: User wants to name the project.
  {"action": "set_project_name", "name": "My Video"}

- mark_range: User describes a time range for a section.
  {"action": "mark_range", "description": "intro", "start_seconds": 0, "end_seconds": 30}

- cut_at: User wants to cut at a specific time.
  {"action": "cut_at", "seconds": 45.5}

- delete_clip: User wants to remove a clip.
  {"action": "delete_clip", "description": "intro"}

- unknown: Can't understand the command.
  {"action": "unknown", "message": "I didn't understand that"}

Parse timestamps like "0:30" as 30 seconds, "1:30" as 90 seconds.

Return ONLY the JSON object, no other text."#;

/// Parse a user command using Ollama
pub async fn parse_command(user_input: &str) -> Result<ParsedAction> {
    let prompt = format!(
        "{}\n\nUser command: {}\n\nJSON response:",
        SYSTEM_PROMPT, user_input
    );

    let request = OllamaRequest {
        model: MODEL.to_string(),
        prompt,
        stream: false,
        format: "json".to_string(),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(OLLAMA_URL)
        .json(&request)
        .send()
        .await
        .context("Failed to connect to Ollama")?;

    if !response.status().is_success() {
        anyhow::bail!("Ollama returned error: {}", response.status());
    }

    let ollama_response: OllamaResponse = response
        .json()
        .await
        .context("Failed to parse Ollama response")?;

    let action: ParsedAction = serde_json::from_str(&ollama_response.response)
        .context("Failed to parse action JSON")?;

    Ok(action)
}

/// Check if Ollama is available
#[allow(dead_code)]
pub async fn is_available() -> bool {
    let client = reqwest::Client::new();
    client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}
