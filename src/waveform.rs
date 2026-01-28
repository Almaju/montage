use gpui::*;

use crate::audio::AudioData;

/// Waveform visualization component
pub struct Waveform {
    audio: AudioData,
}

impl Waveform {
    pub fn new(audio: AudioData) -> Self {
        Self { audio }
    }
}

impl Render for Waveform {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let samples = self.audio.samples.clone();

        div()
            .w_full()
            .h_32()
            .bg(rgb(0x2a2a2a))
            .rounded_md()
            .overflow_hidden()
            .child(
                canvas(
                    move |_bounds, _window, _cx| {},
                    move |bounds, _state, window, _cx| {
                        let width: f32 = bounds.size.width.into();
                        let height: f32 = bounds.size.height.into();
                        let center_y = height / 2.0;
                        let max_amplitude = height / 2.0 - 4.0;

                        let sample_count = samples.len();
                        if sample_count == 0 || width <= 0.0 {
                            return;
                        }

                        let bar_width = 2.0_f32;
                        let bar_gap = 1.0_f32;
                        let bar_step = bar_width + bar_gap;
                        let num_bars = (width / bar_step) as usize;

                        let waveform_color = rgb(0x4fc3f7);
                        let origin_x: f32 = bounds.origin.x.into();
                        let origin_y: f32 = bounds.origin.y.into();

                        for i in 0..num_bars {
                            let x = i as f32 * bar_step;
                            let sample_idx = ((x / width) * sample_count as f32) as usize;
                            let sample_idx = sample_idx.min(sample_count - 1);
                            
                            // Get average of nearby samples for smoother look
                            let range_start = sample_idx.saturating_sub(2);
                            let range_end = (sample_idx + 3).min(sample_count);
                            let avg_sample: f32 = samples[range_start..range_end]
                                .iter()
                                .sum::<f32>()
                                / (range_end - range_start) as f32;

                            let bar_height = (avg_sample * max_amplitude).max(1.0);

                            let bar_bounds = Bounds {
                                origin: point(
                                    px(origin_x + x),
                                    px(origin_y + center_y - bar_height),
                                ),
                                size: size(px(bar_width), px(bar_height * 2.0)),
                            };

                            window.paint_quad(fill(bar_bounds, waveform_color));
                        }
                    },
                )
                .size_full(),
            )
    }
}

/// Timeline component with waveform and time markers
pub struct Timeline {
    duration: f64,
    waveform: Entity<Waveform>,
}

impl Timeline {
    pub fn new(audio: AudioData, cx: &mut Context<Self>) -> Self {
        let duration = audio.duration;
        let waveform = cx.new(|_cx| Waveform::new(audio));
        Self { duration, waveform }
    }
}

impl Render for Timeline {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let duration_str = format_duration(self.duration);

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                // Time markers
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child("0:00")
                    .child(duration_str),
            )
            .child(self.waveform.clone())
    }
}

fn format_duration(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    format!("{}:{:02}", mins, secs)
}
