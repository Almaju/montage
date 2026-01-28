use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::project::Project;

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const MODEL: &str = "qwen2.5:3b";

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

/// Response from the agent with project modifications
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentResponse {
    /// What the agent wants to say to the user
    pub message: String,
    
    /// Project modifications to apply
    #[serde(default)]
    pub modifications: Vec<Modification>,
}

/// A modification to apply to the project
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Modification {
    /// Set the project name
    SetName { name: String },
    
    /// Add a clip to the project
    AddClip {
        description: String,
        /// Path will be filled by the UI if user attaches a file
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        media_type: Option<String>,
    },
    
    /// Remove a clip by ID or description
    RemoveClip {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        description: Option<String>,
    },
    
    /// Update clip description
    UpdateClip {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        old_description: Option<String>,
        new_description: String,
    },
    
    /// Move a clip to a new position (1-indexed)
    MoveClip {
        /// Clip to move (by description)
        description: String,
        /// New position (1-indexed, "first", "last", or number)
        position: String,
    },
    
    /// Swap two clips
    SwapClips {
        /// First clip description
        clip1: String,
        /// Second clip description
        clip2: String,
    },
    
    /// Add a marker/note at a timestamp
    AddMarker {
        description: String,
        #[serde(default)]
        time_seconds: Option<f64>,
    },
    
    /// Set project description
    SetDescription { description: String },
}

const SYSTEM_PROMPT: &str = r#"You are an AI video editing assistant. You help users organize their video projects.

You receive the current project state as JSON and user commands. You respond with:
1. A friendly message to the user
2. A list of modifications to apply to the project

## Response Format (JSON only)
{
  "message": "Your response to the user",
  "modifications": [
    { "type": "set_name", "name": "New Project Name" },
    { "type": "add_clip", "description": "intro sequence" },
    { "type": "remove_clip", "description": "old intro" },
    { "type": "update_clip", "old_description": "clip1", "new_description": "opening shot" },
    { "type": "move_clip", "description": "intro", "position": "last" },
    { "type": "swap_clips", "clip1": "intro", "clip2": "outro" },
    { "type": "add_marker", "description": "cut here", "time_seconds": 30.5 },
    { "type": "set_description", "description": "My vacation video" }
  ]
}

## Modification Types
- set_name: Change project name
- add_clip: Add a new clip (user will attach the file)
- remove_clip: Remove a clip by id or description
- update_clip: Change a clip's description
- move_clip: Move a clip to a new position ("first", "last", or a number like "2")
- swap_clips: Swap the positions of two clips
- add_marker: Add a timestamp marker/note
- set_description: Set project description

## Rules
- Be helpful and conversational in your message
- Only include modifications that the user actually requested
- If the user just asks a question, respond with empty modifications: []
- If adding a clip, just set the description - the user will attach the file
- Keep messages concise
- Clips are ordered in the sequence they will appear in the final video

Return ONLY valid JSON, no other text."#;

/// Process a user command with project context (blocking - runs in thread)
pub fn process_command_blocking(project: &Project, user_input: &str, has_attachments: bool) -> Result<AgentResponse> {
    // Serialize project to give context
    let project_json = serde_json::to_string_pretty(project)
        .context("Failed to serialize project")?;
    
    let attachment_note = if has_attachments {
        "\n\n[User has attached file(s) to this message]"
    } else {
        ""
    };
    
    let prompt = format!(
        "{}\n\n## Current Project State\n```json\n{}\n```\n\n## User Command\n{}{}\n\n## Your Response (JSON only)",
        SYSTEM_PROMPT, project_json, user_input, attachment_note
    );

    let request = OllamaRequest {
        model: MODEL.to_string(),
        prompt,
        stream: false,
        format: "json".to_string(),
    };

    // Use blocking client to avoid Tokio runtime conflict with GPUI
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(OLLAMA_URL)
        .json(&request)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .context("Failed to connect to Ollama. Is it running? (ollama serve)")?;

    if !response.status().is_success() {
        anyhow::bail!("Ollama returned error: {}", response.status());
    }

    let ollama_response: OllamaResponse = response
        .json()
        .context("Failed to parse Ollama response")?;

    tracing::debug!("Ollama raw response: {}", ollama_response.response);

    let agent_response: AgentResponse = serde_json::from_str(&ollama_response.response)
        .context("Failed to parse agent response JSON")?;

    Ok(agent_response)
}

/// Apply modifications to a project
pub fn apply_modifications(project: &mut Project, modifications: &[Modification]) -> Vec<String> {
    let mut results = Vec::new();
    
    for modification in modifications {
        match modification {
            Modification::SetName { name } => {
                project.metadata.name = name.clone();
                results.push(format!("✓ Project renamed to '{}'", name));
            }
            
            Modification::AddClip { description, path, media_type } => {
                if let Some(path_str) = path {
                    let path = std::path::PathBuf::from(path_str);
                    project.add_clip(description.clone(), path);
                    results.push(format!("✓ Added clip: {}", description));
                } else {
                    // Clip added without file - mark as placeholder
                    results.push(format!("📎 Ready to add clip: {} (attach a file)", description));
                }
                let _ = media_type; // For future use
            }
            
            Modification::RemoveClip { id, description } => {
                let initial_len = project.clips.len();
                
                if let Some(clip_id) = id {
                    project.clips.retain(|c| c.id != *clip_id);
                } else if let Some(desc) = description {
                    let desc_lower = desc.to_lowercase();
                    project.clips.retain(|c| !c.description.to_lowercase().contains(&desc_lower));
                }
                
                let removed = initial_len - project.clips.len();
                if removed > 0 {
                    results.push(format!("✓ Removed {} clip(s)", removed));
                } else {
                    results.push("⚠ No matching clips found to remove".to_string());
                }
            }
            
            Modification::UpdateClip { id, old_description, new_description } => {
                let mut updated = false;
                
                for clip in &mut project.clips {
                    let matches = id.as_ref().is_some_and(|i| clip.id == *i)
                        || old_description.as_ref().is_some_and(|d| 
                            clip.description.to_lowercase().contains(&d.to_lowercase())
                        );
                    
                    if matches {
                        clip.description = new_description.clone();
                        updated = true;
                        break;
                    }
                }
                
                if updated {
                    results.push(format!("✓ Updated clip to: {}", new_description));
                } else {
                    results.push("⚠ No matching clip found to update".to_string());
                }
            }
            
            Modification::MoveClip { description, position } => {
                let desc_lower = description.to_lowercase();
                if let Some(idx) = project.clips.iter().position(|c| 
                    c.description.to_lowercase().contains(&desc_lower)
                ) {
                    let clip = project.clips.remove(idx);
                    let new_pos = match position.to_lowercase().as_str() {
                        "first" | "1" | "start" | "beginning" => 0,
                        "last" | "end" => project.clips.len(),
                        s => s.parse::<usize>().unwrap_or(project.clips.len()).saturating_sub(1),
                    };
                    let new_pos = new_pos.min(project.clips.len());
                    project.clips.insert(new_pos, clip);
                    results.push(format!("✓ Moved '{}' to position {}", description, new_pos + 1));
                } else {
                    results.push(format!("⚠ Clip '{}' not found", description));
                }
            }
            
            Modification::SwapClips { clip1, clip2 } => {
                let clip1_lower = clip1.to_lowercase();
                let clip2_lower = clip2.to_lowercase();
                
                let idx1 = project.clips.iter().position(|c| 
                    c.description.to_lowercase().contains(&clip1_lower)
                );
                let idx2 = project.clips.iter().position(|c| 
                    c.description.to_lowercase().contains(&clip2_lower)
                );
                
                if let (Some(i1), Some(i2)) = (idx1, idx2) {
                    project.clips.swap(i1, i2);
                    results.push(format!("✓ Swapped '{}' and '{}'", clip1, clip2));
                } else {
                    results.push("⚠ Could not find both clips to swap".to_string());
                }
            }
            
            Modification::AddMarker { description, time_seconds } => {
                // TODO: Add proper marker support to project
                let time_str = time_seconds
                    .map(|t| format!(" at {:.1}s", t))
                    .unwrap_or_default();
                results.push(format!("📍 Marker{}: {}", time_str, description));
            }
            
            Modification::SetDescription { description } => {
                project.metadata.description = description.clone();
                results.push("✓ Project description updated".to_string());
            }
        }
    }
    
    results
}
