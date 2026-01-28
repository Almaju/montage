use gpui::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::AudioData;

/// Waveform visualization component with playhead
pub struct Waveform {
    audio: AudioData,
    /// Cached bounds for click calculation
    bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// Current playhead position (0.0 to 1.0)
    position: f64,
}

impl Waveform {
    pub fn new(audio: AudioData) -> Self {
        Self {
            audio,
            bounds: Arc::new(Mutex::new(None)),
            position: 0.0,
        }
    }

    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.0, 1.0);
    }
}

impl Render for Waveform {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let samples = self.audio.samples.clone();
        let position = self.position;
        let bounds_for_paint = self.bounds.clone();
        let bounds_for_click = self.bounds.clone();

        div()
            .id("waveform")
            .w_full()
            .h_32()
            .bg(rgb(0x2a2a2a))
            .rounded_md()
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                // Get cached bounds and calculate relative position
                if let Some(bounds) = *bounds_for_click.lock().unwrap() {
                    let click_x: f32 = event.position.x.into();
                    let origin_x: f32 = bounds.origin.x.into();
                    let width: f32 = bounds.size.width.into();
                    
                    let relative_x = click_x - origin_x;
                    let normalized = (relative_x / width).clamp(0.0, 1.0) as f64;
                    
                    this.position = normalized;
                    cx.notify();
                    cx.emit(WaveformEvent::Seek(normalized));
                }
            }))
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        // Store bounds for click calculation
                        *bounds_for_paint.lock().unwrap() = Some(bounds);
                    },
                    move |bounds, _state, window, _cx| {
                        let width: f32 = bounds.size.width.into();
                        let height: f32 = bounds.size.height.into();
                        let center_y = height / 2.0;
                        let max_amplitude = height / 2.0 - 4.0;
                        let origin_x: f32 = bounds.origin.x.into();
                        let origin_y: f32 = bounds.origin.y.into();

                        let sample_count = samples.len();
                        if sample_count == 0 || width <= 0.0 {
                            return;
                        }

                        let bar_width = 2.0_f32;
                        let bar_gap = 1.0_f32;
                        let bar_step = bar_width + bar_gap;
                        let num_bars = (width / bar_step) as usize;

                        let waveform_color = rgb(0x4fc3f7);
                        let played_color = rgb(0x81d4fa);
                        let playhead_x = position as f32 * width;

                        // Draw waveform bars
                        for i in 0..num_bars {
                            let x = i as f32 * bar_step;
                            let sample_idx = ((x / width) * sample_count as f32) as usize;
                            let sample_idx = sample_idx.min(sample_count - 1);

                            let range_start = sample_idx.saturating_sub(2);
                            let range_end = (sample_idx + 3).min(sample_count);
                            let avg_sample: f32 = samples[range_start..range_end]
                                .iter()
                                .sum::<f32>()
                                / (range_end - range_start) as f32;

                            let bar_height = (avg_sample * max_amplitude).max(1.0);

                            // Color bars before playhead differently
                            let color = if x < playhead_x {
                                played_color
                            } else {
                                waveform_color
                            };

                            let bar_bounds = Bounds {
                                origin: point(
                                    px(origin_x + x),
                                    px(origin_y + center_y - bar_height),
                                ),
                                size: size(px(bar_width), px(bar_height * 2.0)),
                            };

                            window.paint_quad(fill(bar_bounds, color));
                        }

                        // Draw playhead line
                        let playhead_bounds = Bounds {
                            origin: point(px(origin_x + playhead_x - 1.0), px(origin_y)),
                            size: size(px(2.0), px(height)),
                        };
                        window.paint_quad(fill(playhead_bounds, rgb(0xffffff)));
                    },
                )
                .size_full(),
            )
    }
}

/// Events emitted by Waveform
pub enum WaveformEvent {
    Seek(f64),
}

impl EventEmitter<WaveformEvent> for Waveform {}

/// Timeline component with waveform, controls, and time display
pub struct Timeline {
    duration: f64,
    /// Whether audio is playing
    playing: bool,
    /// Current position in seconds
    position: f64,
    waveform: Entity<Waveform>,
}

impl Timeline {
    pub fn new(audio: AudioData, cx: &mut Context<Self>) -> Self {
        let duration = audio.duration;
        let waveform = cx.new(|_cx| Waveform::new(audio));

        // Subscribe to waveform events
        cx.subscribe(&waveform, |this, _waveform, event: &WaveformEvent, cx| match event {
            WaveformEvent::Seek(position) => {
                this.seek(*position);
                cx.notify();
            }
        })
        .detach();

        Self {
            duration,
            playing: false,
            position: 0.0,
            waveform,
        }
    }

    fn seek(&mut self, normalized_position: f64) {
        self.position = normalized_position * self.duration;
    }

    fn start_playback_timer(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;

                let should_continue = this
                    .update(cx, |this, cx| {
                        if !this.playing {
                            return false;
                        }

                        this.position += 0.05; // 50ms increment
                        if this.position >= this.duration {
                            this.position = 0.0;
                            this.playing = false;
                            cx.notify();
                            return false;
                        }

                        // Update waveform position
                        let normalized = this.position / this.duration;
                        this.waveform.update(cx, |waveform, cx| {
                            waveform.set_position(normalized);
                            cx.notify();
                        });

                        cx.notify();
                        true
                    })
                    .unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        self.playing = !self.playing;
        if self.playing {
            self.start_playback_timer(cx);
        }
        cx.notify();
    }
}

impl Render for Timeline {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_time = format_duration(self.position);
        let duration_str = format_duration(self.duration);
        let is_playing = self.playing;

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            // Controls row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    // Play/Pause button
                    .child(
                        div()
                            .id("play-pause")
                            .w_10()
                            .h_10()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0x4fc3f7))
                            .rounded_full()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x81d4fa)))
                            .active(|s| s.bg(rgb(0x29b6f6)))
                            .child(if is_playing { "⏸" } else { "▶" })
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.toggle_playback(cx);
                            })),
                    )
                    // Time display
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{} / {}", current_time, duration_str)),
                    ),
            )
            // Waveform
            .child(self.waveform.clone())
            // Time markers below waveform
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(0x666666))
                    .child("0:00")
                    .child(duration_str),
            )
    }
}

fn format_duration(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    format!("{}:{:02}", mins, secs)
}
