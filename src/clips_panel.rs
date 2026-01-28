use gpui::*;
use crate::project::{Clip, MediaType};

/// Events emitted by the clips panel
pub enum ClipsPanelEvent {
    /// User selected a clip
    SelectClip(String),
    /// User wants to delete a clip
    DeleteClip(String),
    /// User wants to move a clip up
    MoveUp(String),
    /// User wants to move a clip down
    MoveDown(String),
}

impl EventEmitter<ClipsPanelEvent> for ClipsPanel {}

/// Panel showing all clips in the project
pub struct ClipsPanel {
    /// Clips to display
    clips: Vec<Clip>,
    /// Currently selected clip ID
    selected_id: Option<String>,
}

impl ClipsPanel {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            selected_id: None,
        }
    }
    
    /// Update the clips list
    pub fn set_clips(&mut self, clips: Vec<Clip>) {
        self.clips = clips;
    }
    
    /// Set the selected clip
    #[allow(dead_code)]
    pub fn set_selected(&mut self, id: Option<String>) {
        self.selected_id = id;
    }
    
    fn render_clip(&self, clip: &Clip, index: usize, total: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let clip_id = clip.id.clone();
        let clip_id_for_select = clip.id.clone();
        let clip_id_for_delete = clip.id.clone();
        let clip_id_for_up = clip.id.clone();
        let clip_id_for_down = clip.id.clone();
        let is_selected = self.selected_id.as_ref() == Some(&clip.id);
        let is_first = index == 0;
        let is_last = index == total - 1;
        
        let icon = match clip.media_type {
            MediaType::Video => "🎬",
            MediaType::Audio => "🎵",
            MediaType::Image => "🖼️",
        };
        
        let file_name = clip.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        
        div()
            .id(SharedString::from(clip.id.clone()))
            .w_full()
            .p_2()
            .mb_1()
            .bg(if is_selected { rgb(0x3a3a3a) } else { rgb(0x2a2a2a) })
            .border_1()
            .border_color(if is_selected { rgb(0x4fc3f7) } else { rgb(0x333333) })
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x333333)))
            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                this.selected_id = Some(clip_id_for_select.clone());
                cx.emit(ClipsPanelEvent::SelectClip(clip_id_for_select.clone()));
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    // Header with icon, description, and controls
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    // Order number
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x555555))
                                            .child(format!("{}.", index + 1))
                                    )
                                    .child(div().text_sm().child(icon))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xffffff))
                                            .overflow_hidden()
                                            .max_w(px(100.0))
                                            .child(if clip.description.is_empty() {
                                                "Untitled".to_string()
                                            } else {
                                                clip.description.clone()
                                            })
                                    )
                            )
                            // Controls: up, down, delete
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    // Move up
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("up-{}", clip_id)))
                                            .text_xs()
                                            .text_color(if is_first { rgb(0x444444) } else { rgb(0x666666) })
                                            .cursor(if is_first { CursorStyle::default() } else { CursorStyle::PointingHand })
                                            .hover(|s| if is_first { s } else { s.text_color(rgb(0x4fc3f7)) })
                                            .child("▲")
                                            .on_click(cx.listener(move |_this, _event: &ClickEvent, _window, cx| {
                                                if !is_first {
                                                    cx.emit(ClipsPanelEvent::MoveUp(clip_id_for_up.clone()));
                                                }
                                            }))
                                    )
                                    // Move down
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("down-{}", clip_id_for_down.clone())))
                                            .text_xs()
                                            .text_color(if is_last { rgb(0x444444) } else { rgb(0x666666) })
                                            .cursor(if is_last { CursorStyle::default() } else { CursorStyle::PointingHand })
                                            .hover(|s| if is_last { s } else { s.text_color(rgb(0x4fc3f7)) })
                                            .child("▼")
                                            .on_click(cx.listener(move |_this, _event: &ClickEvent, _window, cx| {
                                                if !is_last {
                                                    cx.emit(ClipsPanelEvent::MoveDown(clip_id_for_down.clone()));
                                                }
                                            }))
                                    )
                                    // Delete
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("delete-{}", clip_id_for_delete.clone())))
                                            .text_xs()
                                            .text_color(rgb(0x666666))
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(rgb(0xff6b6b)))
                                            .child("×")
                                            .on_click(cx.listener(move |_this, _event: &ClickEvent, _window, cx| {
                                                cx.emit(ClipsPanelEvent::DeleteClip(clip_id_for_delete.clone()));
                                            }))
                                    )
                            )
                    )
                    // File name
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .overflow_hidden()
                            .child(file_name)
                    )
            )
    }
}

impl Render for ClipsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pre-render clips to avoid closure lifetime issues
        let total = self.clips.len();
        let clip_elements: Vec<AnyElement> = self.clips
            .iter()
            .enumerate()
            .map(|(i, c)| self.render_clip(c, i, total, cx).into_any_element())
            .collect();
        let clips_count = total;
        
        div()
            .h_full()
            .w(px(200.0))
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .border_r_1()
            .border_color(rgb(0x333333))
            // Header
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_3()
                    .border_b_1()
                    .border_color(rgb(0x333333))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x888888))
                            .child("CLIPS")
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child(format!("{}", clips_count))
                    )
            )
            // Clips list
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .p_2()
                    .child(if clip_elements.is_empty() {
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .text_sm()
                            .text_color(rgb(0x555555))
                            .child("No clips yet")
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .children(clip_elements)
                            .into_any_element()
                    })
            )
    }
}
