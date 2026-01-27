use gpui::*;

struct MontageApp;

impl MontageApp {
    fn new() -> Self {
        Self
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    
    App::new().run(|cx: &mut AppContext| {
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Montage".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |cx| {
                cx.new_view(|_cx| MainView::new())
            },
        )
        .unwrap();
    });
}

struct MainView {
    status: String,
}

impl MainView {
    fn new() -> Self {
        Self {
            status: "Welcome to Montage! 🎬".to_string(),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .child(
                div()
                    .p_4()
                    .text_xl()
                    .child("Montage")
            )
            .child(
                div()
                    .flex_1()
                    .p_4()
                    .child(&self.status)
            )
            .child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(0x888888))
                    .child("Phase 1: Foundation — Building the UI shell")
            )
    }
}
