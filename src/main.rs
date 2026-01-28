mod audio;
mod waveform;

use audio::AudioData;
use gpui::*;
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

    fn open_file_picker(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Audio File".into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| {
                        this.load_audio(path, cx);
                    });
                }
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
                            .id("open-btn")
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
                                this.open_file_picker(cx);
                            })),
                    ),
            )
            // Main content
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_8()
                    .child(match &self.state {
                        AppState::Empty => self.render_empty(cx).into_any_element(),
                        AppState::Error(msg) => self.render_error(msg).into_any_element(),
                        AppState::Loaded { timeline } => {
                            self.render_loaded(timeline.clone()).into_any_element()
                        }
                        AppState::Loading => self.render_loading().into_any_element(),
                    }),
            )
            // Footer
            .child(
                div()
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(0x333333))
                    .text_sm()
                    .text_color(rgb(0x666666))
                    .child("Phase 1: Foundation — Audio loading & waveform display"),
            )
    }
}

impl MainView {
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .child(
                div()
                    .text_2xl()
                    .child("🎵"),
            )
            .child(
                div()
                    .text_xl()
                    .text_color(rgb(0x888888))
                    .child("Drop an audio file or click Open Audio"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x555555))
                    .child("Supports MP3, WAV, FLAC, OGG, and more"),
            )
            // Drop zone
            .id("drop-zone")
            .w_full()
            .max_w(px(600.0))
            .p_12()
            .border_2()
            .border_color(rgb(0x333333))
            .rounded_xl()
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(0x4fc3f7)).bg(rgb(0x1e1e1e)))
            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                this.open_file_picker(cx);
            }))
    }

    fn render_error(&self, msg: &str) -> impl IntoElement {
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
            )
    }

    fn render_loaded(&self, timeline: Entity<Timeline>) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_4()
            .child(timeline)
    }

    fn render_loading(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .child(div().text_2xl().child("⏳"))
            .child(div().text_lg().text_color(rgb(0x888888)).child("Loading audio..."))
    }
}
