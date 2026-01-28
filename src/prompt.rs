use gpui::*;
use std::path::PathBuf;

/// Events emitted by the prompt input
pub enum PromptEvent {
    /// User submitted a command with optional file attachments
    Submit {
        text: String,
        attachments: Vec<PathBuf>,
    },
}

impl EventEmitter<PromptEvent> for PromptInput {}

/// Prompt input component for agentic interactions
pub struct PromptInput {
    /// Current input text
    text: String,
    /// Attached files (via @ or drag-drop)
    attachments: Vec<Attachment>,
    /// Whether the input is focused
    focused: bool,
}

#[derive(Clone)]
pub struct Attachment {
    pub name: String,
    pub path: PathBuf,
}

impl PromptInput {
    pub fn new() -> Self {
        Self {
            attachments: Vec::new(),
            focused: false,
            text: String::new(),
        }
    }

    /// Add a file attachment
    pub fn attach_file(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());
        
        self.attachments.push(Attachment { name, path });
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.text.clear();
        self.attachments.clear();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.text.trim().is_empty() && self.attachments.is_empty() {
            return;
        }

        let text = self.text.clone();
        let attachments = self.attachments.iter().map(|a| a.path.clone()).collect();
        
        cx.emit(PromptEvent::Submit { text, attachments });
        self.clear();
        cx.notify();
    }

    fn render_attachment(&self, attachment: &Attachment, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let idx = index;
        div()
            .id(("attachment", index))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(rgb(0x3a3a3a))
            .rounded_md()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x4fc3f7))
                    .child("📎"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0xcccccc))
                    .child(attachment.name.clone()),
            )
            .child(
                div()
                    .id(("remove-attachment", index))
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(0xff6b6b)))
                    .child("×")
                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                        this.attachments.remove(idx);
                        cx.notify();
                    })),
            )
    }
}

impl Render for PromptInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_attachments = !self.attachments.is_empty();
        
        // Pre-render attachments to avoid closure lifetime issues
        let attachment_elements: Vec<AnyElement> = self.attachments
            .iter()
            .enumerate()
            .map(|(i, a)| self.render_attachment(a, i, cx).into_any_element())
            .collect();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            // Attachments row (if any)
            .child(if has_attachments {
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(attachment_elements)
                    .into_any_element()
            } else {
                div().into_any_element()
            })
            // Input row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_3()
                    .bg(rgb(0x2a2a2a))
                    .border_1()
                    .border_color(if self.focused { rgb(0x4fc3f7) } else { rgb(0x3a3a3a) })
                    .rounded_lg()
                    // Attach button
                    .child(
                        div()
                            .id("attach-btn")
                            .px_2()
                            .py_1()
                            .text_color(rgb(0x888888))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(0x4fc3f7)))
                            .child("📎")
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.open_file_picker(cx);
                            })),
                    )
                    // Text input area
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_color(if self.text.is_empty() { rgb(0x666666) } else { rgb(0xffffff) })
                                    .child(if self.text.is_empty() {
                                        "Describe what to add... (attach files with 📎 or drag & drop)".to_string()
                                    } else {
                                        self.text.clone()
                                    }),
                            ),
                    )
                    // Submit button
                    .child(
                        div()
                            .id("submit-btn")
                            .px_3()
                            .py_1()
                            .bg(rgb(0x4fc3f7))
                            .text_color(rgb(0x000000))
                            .font_weight(FontWeight::MEDIUM)
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x81d4fa)))
                            .child("→")
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.submit(cx);
                            })),
                    ),
            )
    }
}

impl PromptInput {
    fn open_file_picker(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            directories: false,
            files: true,
            multiple: true,
            prompt: Some("Attach files".into()),
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await {
                let _ = this.update(cx, |this, cx| {
                    for path in paths {
                        this.attach_file(path);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

/// Parse a command and extract intent
#[derive(Debug)]
pub enum Command {
    /// Add media to the project with a description
    AddMedia {
        description: String,
        files: Vec<PathBuf>,
    },
    /// Set project name
    SetName(String),
    /// Unknown command
    Unknown(String),
}

impl Command {
    /// Parse a prompt submission into a command
    pub fn parse(text: &str, attachments: Vec<PathBuf>) -> Self {
        let text = text.trim();
        
        // If there are attachments, treat as AddMedia
        if !attachments.is_empty() {
            return Self::AddMedia {
                description: text.to_string(),
                files: attachments,
            };
        }
        
        // Check for specific commands
        if let Some(name) = text.strip_prefix("name:").or_else(|| text.strip_prefix("project:")) {
            return Self::SetName(name.trim().to_string());
        }
        
        // Default: unknown
        Self::Unknown(text.to_string())
    }
}
