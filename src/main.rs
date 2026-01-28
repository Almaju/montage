mod audio;
mod video;
mod waveform;

use audio::AudioData;
use gpui::*;
use video::VideoPlayer;
use waveform::Timeline;

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
    state: AppState,
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
            state: AppState::Empty,
            video_player: None,
        }
    }

    fn load_audio(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.state = AppState::Loading;
        cx.notify();

        let path_clone = path.clone();
        cx.spawn(async move |this, cx| {
            let result = std::thread::spawn(move || AudioData::load(&path_clone)).join();

            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(audio)) => {
                        let timeline = cx.new(|cx| Timeline::new(audio, cx));
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
                            .gap_2()
                            .child("🎬")
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child("Montage")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
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
                                    .child("Open Video")
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
                                    .child("Open Audio")
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
