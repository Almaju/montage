mod audio;
mod project;
mod video;
mod waveform;

use audio::AudioData;
use gpui::*;
use project::Project;
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
    /// Current project
    project: Project,
    /// Path to the current project file (if saved)
    project_path: Option<std::path::PathBuf>,
    /// App state
    state: AppState,
    /// Video player instance
    video_player: Option<VideoPlayer>,
}

enum AppState {
    Empty,
    Error(String),
    Loaded { timeline: Entity<Timeline> },
    Loading,
}

impl MainView {
    fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            project: Project::new("Untitled"),
            project_path: None,
            state: AppState::Empty,
            video_player: None,
        }
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
                    match Project::load(&path) {
                        Ok(project) => {
                            this.project = project;
                            this.project_path = Some(path);
                            this.state = AppState::Empty;
                            
                            // Load audio if specified in project
                            if let Some(ref audio) = this.project.audio {
                                this.load_audio(audio.path.clone(), cx);
                            }
                            
                            // Load video if specified in project
                            if let Some(ref video) = this.project.video {
                                this.load_video(video.path.clone(), cx);
                            }
                        }
                        Err(e) => {
                            this.state = AppState::Error(format!("Failed to open: {}", e));
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

    fn open_video_picker(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Video File".into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.load_video(path, cx);
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
                            // Project buttons
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
                            // Media buttons
                            .child(
                                div()
                                    .id("open-video-btn")
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(0x9c27b0))
                                    .text_color(rgb(0xffffff))
                                    .font_weight(FontWeight::MEDIUM)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0xba68c8)))
                                    .active(|s| s.bg(rgb(0x7b1fa2)))
                                    .child("Video")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.open_video_picker(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("open-audio-btn")
                                    .px_4()
                                    .py_2()
                                    .bg(rgb(0x4fc3f7))
                                    .text_color(rgb(0x000000))
                                    .font_weight(FontWeight::MEDIUM)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x81d4fa)))
                                    .active(|s| s.bg(rgb(0x29b6f6)))
                                    .child("Audio")
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.open_audio_picker(cx);
                                    })),
                            ),
                    ),
            )
            // Main content area
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
            )
            // Footer
            .child(
                div()
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(0x333333))
                    .text_sm()
                    .text_color(rgb(0x666666))
                    .child("Phase 2: Video + Audio integration"),
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
