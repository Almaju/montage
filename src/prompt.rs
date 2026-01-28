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
    /// Focus handle for keyboard input
    focus_handle: FocusHandle,
    /// Whether we're processing a command
    processing: bool,
}

#[derive(Clone)]
pub struct Attachment {
    pub name: String,
    pub path: PathBuf,
}

impl PromptInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            attachments: Vec::new(),
            focus_handle: cx.focus_handle(),
            text: String::new(),
            processing: false,
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
    
    /// Set processing state
    pub fn set_processing(&mut self, processing: bool) {
        self.processing = processing;
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.text.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        
        if self.processing {
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

impl Focusable for PromptInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PromptInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_attachments = !self.attachments.is_empty();
        let is_focused = self.focus_handle.is_focused(window);
        
        // Pre-render attachments to avoid closure lifetime issues
        let attachment_elements: Vec<AnyElement> = self.attachments
            .iter()
            .enumerate()
            .map(|(i, a)| self.render_attachment(a, i, cx).into_any_element())
            .collect();
        
        let placeholder = if self.processing {
            "Processing..."
        } else {
            "Type a command... (e.g., 'this is the intro' or 'cut at 0:30')"
        };

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
                    .id("prompt-input-row")
                    .track_focus(&self.focus_handle)
                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                        this.focus_handle.focus(window, cx);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                        match &event.keystroke.key {
                            key if key == "enter" => {
                                this.submit(cx);
                            }
                            key if key == "backspace" => {
                                this.text.pop();
                                cx.notify();
                            }
                            key if key == "escape" => {
                                this.text.clear();
                                cx.notify();
                            }
                            _ => {
                                // Handle regular character input via key_char
                                if let Some(ch) = &event.keystroke.key_char {
                                    this.text.push_str(ch);
                                    cx.notify();
                                }
                            }
                        }
                    }))
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_3()
                    .bg(rgb(0x2a2a2a))
                    .border_1()
                    .border_color(if is_focused { rgb(0x4fc3f7) } else { rgb(0x3a3a3a) })
                    .rounded_lg()
                    .cursor_text()
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
                            .min_h(px(20.0))
                            .child(
                                div()
                                    .text_color(if self.text.is_empty() { rgb(0x666666) } else { rgb(0xffffff) })
                                    .child(if self.text.is_empty() {
                                        format!("{}{}", placeholder, if is_focused { "│" } else { "" })
                                    } else {
                                        format!("{}{}", self.text.clone(), if is_focused { "│" } else { "" })
                                    }),
                            ),
                    )
                    // Submit button
                    .child(
                        div()
                            .id("submit-btn")
                            .px_3()
                            .py_1()
                            .bg(if self.processing { rgb(0x666666) } else { rgb(0x4fc3f7) })
                            .text_color(rgb(0x000000))
                            .font_weight(FontWeight::MEDIUM)
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| if self.processing { s } else { s.bg(rgb(0x81d4fa)) })
                            .child(if self.processing { "⏳" } else { "→" })
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.submit(cx);
                            })),
                    ),
            )
    }
}

/// Parse a command and extract intent (fallback when Ollama unavailable)
#[derive(Debug)]
pub enum Command {
    /// Add media to the project with a description
    AddMedia {
        description: String,
        files: Vec<PathBuf>,
    },
    /// Set project name
    SetName(String),
    /// Text command to be parsed by Ollama
    Text(String),
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
        
        // If we have text but no attachments, send to Ollama
        if !text.is_empty() {
            return Self::Text(text.to_string());
        }
        
        // Default: unknown
        Self::Unknown(text.to_string())
    }
}
