mod agent;
mod audio;
mod clips_panel;
mod config;
mod export;
mod project;
mod prompt;
mod video;
mod waveform;

use audio::AudioData;
use clips_panel::{ClipsPanel, ClipsPanelEvent};
use config::AppConfig;
use gpui::*;
use project::Project;
use prompt::{PromptEvent, PromptInput};
use video::VideoPlayer;
use waveform::{Timeline, TimelineEvent};

fn main() {
    tracing_subscriber::fmt::init();

    Application::new().run(|cx| {
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Montage".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(100.0), px(100.0)),
                    size: size(px(1200.0), px(800.0)),
                })),
                focus: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| MainView::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}

struct MainView {
    /// App configuration (persisted)
    config: AppConfig,
    /// Current project
    project: Project,
    /// Path to the current project file (if saved)
    project_path: Option<std::path::PathBuf>,
    /// Clips panel showing all clips
    clips_panel: Entity<ClipsPanel>,
    /// Prompt input for agentic interactions
    prompt: Entity<PromptInput>,
    /// App state
    state: AppState,
    /// Video player instance
    video_player: Option<VideoPlayer>,
    /// Last message from the agent
    last_agent_message: Option<String>,
    /// Last modification results from the agent
    last_agent_results: Vec<String>,
}

enum AppState {
    Empty,
    Error(String),
    Loaded { timeline: Entity<Timeline> },
    Loading,
}

impl MainView {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = AppConfig::load();
        let clips_panel = cx.new(|_cx| ClipsPanel::new());
        let prompt = cx.new(PromptInput::new);
        
        // Subscribe to clips panel events
        cx.subscribe(&clips_panel, |this, _panel, event: &ClipsPanelEvent, cx| {
            match event {
                ClipsPanelEvent::SelectClip(id) => {
                    tracing::info!("Selected clip: {}", id);
                    // TODO: Load clip into preview
                }
                ClipsPanelEvent::DeleteClip(id) => {
                    this.project.clips.retain(|c| c.id != *id);
                    this.sync_clips_panel(cx);
                    this.last_agent_message = Some("Clip deleted".to_string());
                    this.last_agent_results = vec![];
                    cx.notify();
                }
            }
        })
        .detach();
        
        // Subscribe to prompt events
        cx.subscribe(&prompt, |this, _prompt, event: &PromptEvent, cx| {
            match event {
                PromptEvent::Submit { text, attachments } => {
                    this.handle_prompt(text.clone(), attachments.clone(), cx);
                }
            }
        })
        .detach();
        
        let mut view = Self {
            config,
            project: Project::new("Untitled"),
            project_path: None,
            clips_panel,
            prompt,
            state: AppState::Empty,
            video_player: None,
            last_agent_message: None,
            last_agent_results: vec![],
        };
        
        // Auto-load last project if exists
        if let Some(ref last_project) = view.config.last_project.clone()
            && last_project.exists()
        {
            tracing::info!("Auto-loading last project: {:?}", last_project);
            view.load_project_from_path(last_project.clone(), cx);
        }
        
        view
    }
    
    /// Load a project from a specific path
    fn load_project_from_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        match Project::load(&path) {
            Ok(project) => {
                self.project = project;
                self.project_path = Some(path.clone());
                self.state = AppState::Empty;
                
                // Update config with this project
                self.config.set_last_project(path);
                
                // Load audio if specified in project
                if let Some(ref audio) = self.project.audio
                    && audio.path.exists()
                {
                    self.load_audio(audio.path.clone(), cx);
                }
                
                // Load video if specified in project
                if let Some(ref video) = self.project.video
                    && video.path.exists()
                {
                    self.load_video(video.path.clone(), cx);
                }
                
                // Sync clips panel
                self.sync_clips_panel(cx);
                
                tracing::info!("Loaded project: {}", self.project.metadata.name);
            }
            Err(e) => {
                tracing::error!("Failed to load project: {}", e);
                self.state = AppState::Error(format!("Failed to open: {}", e));
            }
        }
        cx.notify();
    }
    
    fn handle_prompt(&mut self, text: String, attachments: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let has_attachments = !attachments.is_empty();
        
        // If we have file attachments, add them directly
        if has_attachments {
            for file in &attachments {
                // Add clip to project with the text as description
                let description = if text.is_empty() {
                    file.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled clip".to_string())
                } else {
                    text.clone()
                };
                
                let clip = self.project.add_clip(description, file.clone());
                let media_type = clip.media_type.clone();
                
                tracing::info!("Added {:?} clip: {}", media_type, clip.description);
                
                // Load the media
                match media_type {
                    project::MediaType::Audio => {
                        self.load_audio(file.clone(), cx);
                    }
                    project::MediaType::Video => {
                        self.load_video(file.clone(), cx);
                    }
                    project::MediaType::Image => {
                        tracing::info!("Image support coming soon");
                    }
                }
            }
            
            self.last_agent_message = Some(format!("Added {} file(s) to project", attachments.len()));
            self.last_agent_results = vec![];
            self.sync_clips_panel(cx);
            cx.notify();
            return;
        }
        
        // If we have text but no attachments, send to agent
        if !text.trim().is_empty() {
            self.process_with_agent(text, has_attachments, cx);
        }
    }
    
    /// Sync the clips panel with the current project
    fn sync_clips_panel(&mut self, cx: &mut Context<Self>) {
        let clips = self.project.clips.clone();
        self.clips_panel.update(cx, |panel, cx| {
            panel.set_clips(clips);
            cx.notify();
        });
    }
    
    fn process_with_agent(&mut self, text: String, has_attachments: bool, cx: &mut Context<Self>) {
        // Set processing state
        self.prompt.update(cx, |prompt, cx| {
            prompt.set_processing(true);
            cx.notify();
        });
        
        tracing::info!("Sending to agent: {}", text);
        
        // Clone project for the blocking task
        let project_clone = self.project.clone();
        
        cx.spawn(async move |this, cx| {
            // Run blocking HTTP request in a separate thread
            let result = std::thread::spawn(move || {
                agent::process_command_blocking(&project_clone, &text, has_attachments)
            }).join();
            
            let _ = this.update(cx, |this, cx| {
                // Clear processing state
                this.prompt.update(cx, |prompt, cx| {
                    prompt.set_processing(false);
                    cx.notify();
                });
                
                match result {
                    Ok(Ok(response)) => {
                        tracing::info!("Agent response: {}", response.message);
                        tracing::info!("Agent modifications: {:?}", response.modifications);
                        
                        // Apply modifications to project
                        let results = agent::apply_modifications(&mut this.project, &response.modifications);
                        for result in &results {
                            tracing::info!("{}", result);
                        }
                        
                        // Store agent message for display
                        this.last_agent_message = Some(response.message);
                        this.last_agent_results = results;
                        
                        // Sync clips panel
                        this.sync_clips_panel(cx);
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Agent error: {}", e);
                        this.last_agent_message = Some(format!("Error: {}", e));
                        this.last_agent_results = vec![];
                    }
                    Err(_) => {
                        tracing::error!("Agent thread panicked");
                        this.last_agent_message = Some("Error: Agent crashed".to_string());
                        this.last_agent_results = vec![];
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    
    fn save_project(&mut self, cx: &mut Context<Self>) {
        if let Some(ref path) = self.project_path {
            // Save to existing path
            if let Err(e) = self.project.save(path) {
                tracing::error!("Failed to save project: {}", e);
                self.state = AppState::Error(format!("Failed to save: {}", e));
                cx.notify();
            }
        } else {
            // Prompt for save location
            self.save_project_as(cx);
        }
    }
    
    fn save_project_as(&mut self, cx: &mut Context<Self>) {
        let suggested_name = format!(
            "{}.{}",
            self.project.metadata.name,
            Project::EXTENSION
        );
        
        // Use home directory as default save location
        let home_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        
        let future = cx.prompt_for_new_path(&home_dir, Some(&suggested_name));
        
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = future.await {
                let _ = this.update(cx, |this, cx| {
                    this.project_path = Some(path.clone());
                    if let Err(e) = this.project.save(&path) {
                        tracing::error!("Failed to save project: {}", e);
                        this.state = AppState::Error(format!("Failed to save: {}", e));
                    } else {
                        // Update config with saved project
                        this.config.set_last_project(path);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }
    
    fn open_project(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Project".into()),
        });
        
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.load_project_from_path(path, cx);
                });
            }
        })
        .detach();
    }
    
    fn start_export(&mut self, cx: &mut Context<Self>) {
        // Check if we have clips to export
        let video_clips: Vec<_> = self.project.clips
            .iter()
            .filter(|c| c.media_type == project::MediaType::Video)
            .collect();
        
        if video_clips.is_empty() {
            self.last_agent_message = Some("No video clips to export. Add some videos first!".to_string());
            self.last_agent_results = vec![];
            cx.notify();
            return;
        }
        
        // Prompt for output location
        let default_name = format!("{}.mp4", self.project.metadata.name);
        let home_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        
        let future = cx.prompt_for_new_path(&home_dir, Some(&default_name));
        let project_clone = self.project.clone();
        
        self.last_agent_message = Some("Starting export...".to_string());
        self.last_agent_results = vec![];
        cx.notify();
        
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(output_path))) = future.await {
                // Run export in a separate thread
                let export_result = std::thread::spawn(move || {
                    let settings = export::ExportSettings {
                        output_path: output_path.clone(),
                        ..Default::default()
                    };
                    
                    export::export_project(&project_clone, &settings, None)
                        .map(|_| output_path)
                }).join();
                
                let _ = this.update(cx, |this, cx| {
                    match export_result {
                        Ok(Ok(path)) => {
                            this.last_agent_message = Some("✅ Export complete!".to_string());
                            this.last_agent_results = vec![format!("Saved to: {}", path.display())];
                        }
                        Ok(Err(e)) => {
                            this.last_agent_message = Some("❌ Export failed".to_string());
                            this.last_agent_results = vec![format!("Error: {}", e)];
                        }
                        Err(_) => {
                            this.last_agent_message = Some("❌ Export crashed".to_string());
                            this.last_agent_results = vec![];
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn load_audio(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.state = AppState::Loading;
        cx.notify();

        let path_for_project = path.clone();
        let path_clone = path.clone();
        cx.spawn(async move |this, cx| {
            let result = std::thread::spawn(move || AudioData::load(&path_clone)).join();

            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(audio)) => {
                        // Update project with audio info
                        this.project.set_audio(
                            path_for_project,
                            audio.duration,
                            audio.sample_rate,
                        );
                        
                        let timeline = cx.new(|cx| Timeline::new(audio, cx));
                        
                        // Subscribe to timeline position changes to sync video
                        cx.subscribe(&timeline, |this, _timeline, event: &TimelineEvent, _cx| {
                            match event {
                                TimelineEvent::PositionChanged(position) => {
                                    this.project.timeline.position = *position;
                                    if let Some(ref player) = this.video_player {
                                        player.seek(*position);
                                    }
                                }
                            }
                        })
                        .detach();
                        
                        this.state = AppState::Loaded { timeline };
                    }
                    Ok(Err(e)) => {
                        this.state = AppState::Error(format!("Failed to load audio: {}", e));
                    }
                    Err(_) => {
                        this.state = AppState::Error("Audio loading panicked".to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_video(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let mut player = VideoPlayer::new();
        match player.load(&path) {
            Ok(()) => {
                let (width, height) = player.dimensions();
                let duration = player.duration();
                tracing::info!(
                    "Video loaded: {}x{}, {:.1}s",
                    width,
                    height,
                    duration
                );
                
                // Update project with video info
                self.project.set_video(path, duration, (width, height));
                
                self.video_player = Some(player);
            }
            Err(e) => {
                tracing::error!("Failed to load video: {}", e);
                self.state = AppState::Error(format!("Failed to load video: {}", e));
            }
        }
        cx.notify();
    }

    fn open_audio_picker(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Audio File".into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.load_audio(path, cx);
                });
            }
        })
        .detach();
    }

}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1a1a1a))
            .text_color(rgb(0xffffff))
            // Header
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(rgb(0x333333))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child("🎬")
                                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("Montage")),
                            )
                            // Project name
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x888888))
                                    .child(format!("— {}", self.project.metadata.name)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            // Project buttons only - media added via prompt
                            .child(
                                div()
                                    .id("open-project-btn")
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0x333333))
                                    .text_color(rgb(0xcccccc))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x444444)))
                                    .child("Open")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.open_project(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("save-project-btn")
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0x333333))
                                    .text_color(rgb(0xcccccc))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x444444)))
                                    .child("Save")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.save_project(cx);
                                    })),
                            )
                            // Separator
                            .child(div().w_px().h_6().bg(rgb(0x444444)))
                            // Export button
                            .child(
                                div()
                                    .id("export-btn")
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(0x4caf50))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(FontWeight::MEDIUM)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x66bb6a)))
                                    .child("Export")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.start_export(cx);
                                    })),
                            ),
                    ),
            )
            // Main content area (clips panel + preview/timeline)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // Clips panel (left sidebar)
                    .child(self.clips_panel.clone())
                    // Video preview and timeline (right side)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Video preview area (top half)
                            .child(self.render_video_preview())
                            // Timeline area (bottom half)
                            .child(
                                div()
                                    .h(px(200.0))
                                    .border_t_1()
                                    .border_color(rgb(0x333333))
                                    .child(match &self.state {
                                        AppState::Empty => self.render_empty(cx).into_any_element(),
                                        AppState::Error(msg) => self.render_error(msg).into_any_element(),
                                        AppState::Loaded { timeline } => timeline.clone().into_any_element(),
                                        AppState::Loading => self.render_loading().into_any_element(),
                                    }),
                            ),
                    ),
            )
            // Prompt input (agentic interface)
            .child(
                div()
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(0x333333))
                    .flex()
                    .flex_col()
                    .gap_2()
                    // Agent response (if any)
                    .child(if let Some(ref msg) = self.last_agent_message {
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_3()
                            .bg(rgb(0x252525))
                            .rounded_md()
                            .border_l_2()
                            .border_color(rgb(0x4fc3f7))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xdddddd))
                                    .child(format!("🤖 {}", msg))
                            )
                            .children(
                                self.last_agent_results.iter().map(|r| {
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x888888))
                                        .child(r.clone())
                                })
                            )
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    // Clips indicator
                    .child(if !self.project.clips.is_empty() {
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child(format!("📁 {} clip(s) in project", self.project.clips.len()))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(self.prompt.clone()),
            )
    }
}

impl MainView {
    fn render_video_preview(&self) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x0d0d0d))
            .child(if let Some(ref player) = self.video_player {
                let (width, height) = player.dimensions();
                let duration = player.duration();
                
                // Try to get the current frame
                if let Some(frame) = player.current_frame() {
                    if let Some(render_image) = frame.to_render_image() {
                        // Display actual video frame
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_2()
                            .child(
                                img(render_image)
                                    .max_w(px(800.0))
                                    .max_h(px(450.0))
                                    .rounded_md(),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x666666))
                                    .child(format!("{}×{} • {:.1}s", width, height, duration)),
                            )
                            .into_any_element()
                    } else {
                        // Frame exists but couldn't convert to image
                        self.render_video_metadata(width, height, duration)
                    }
                } else {
                    // No frame yet, show metadata
                    self.render_video_metadata(width, height, duration)
                }
            } else {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .text_3xl()
                            .text_color(rgb(0x333333))
                            .child("📹"),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x555555))
                            .child("No video loaded"),
                    )
                    .into_any_element()
            })
    }
    
    fn render_video_metadata(&self, width: u32, height: u32, duration: f64) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(
                div()
                    .text_3xl()
                    .child("🎬"),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(0x4fc3f7))
                    .child(format!("Video: {}×{}", width, height)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x888888))
                    .child(format!("Duration: {:.1}s", duration)),
            )
            .into_any_element()
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .text_2xl()
                            .text_color(rgb(0x333333))
                            .child("🎵"),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x555555))
                            .child("Load audio to see waveform"),
                    )
                    .id("audio-drop-zone")
                    .p_8()
                    .border_2()
                    .border_color(rgb(0x333333))
                    .rounded_lg()
                    .cursor_pointer()
                    .hover(|s| s.border_color(rgb(0x4fc3f7)).bg(rgb(0x1e1e1e)))
                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                        this.open_audio_picker(cx);
                    })),
            )
    }

    fn render_error(&self, msg: &str) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(div().text_2xl().child("❌"))
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(0xff6b6b))
                            .child(msg.to_string()),
                    ),
            )
    }

    fn render_loading(&self) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(div().text_2xl().child("⏳"))
                    .child(div().text_lg().text_color(rgb(0x888888)).child("Loading...")),
            )
    }
}
