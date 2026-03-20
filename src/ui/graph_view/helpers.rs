//! Text and icon helpers for graph rendering.

use crate::domain::graph::MediaClass;

/// Returns a Phosphor icon string for a given media class, or None for unknown/other.
pub(super) fn media_class_icon(media_class: Option<&MediaClass>) -> Option<&'static str> {
    match media_class {
        Some(MediaClass::AudioSource) => Some(egui_phosphor::regular::MICROPHONE),
        Some(MediaClass::AudioSink) => Some(egui_phosphor::regular::SPEAKER_HIGH),
        Some(MediaClass::StreamInputAudio) => Some(egui_phosphor::regular::WAVEFORM),
        Some(MediaClass::StreamOutputAudio) => Some(egui_phosphor::regular::WAVEFORM),
        Some(MediaClass::MidiSource) | Some(MediaClass::MidiSink) => {
            Some(egui_phosphor::regular::PIANO_KEYS)
        }
        Some(MediaClass::VideoSource)
        | Some(MediaClass::VideoSink)
        | Some(MediaClass::VideoDevice) => Some(egui_phosphor::regular::MONITOR_PLAY),
        Some(MediaClass::AudioDevice) => Some(egui_phosphor::regular::SPEAKER_HIGH),
        Some(MediaClass::AudioVideoSource) => Some(egui_phosphor::regular::MONITOR_PLAY),
        _ => None,
    }
}

/// Truncates a string to fit within a maximum width in pixels using actual font measurement.
/// Uses smart truncation: shows beginning and end of text (e.g., "playback_F..._FL")
/// to keep distinguishing characters visible.
pub(super) fn truncate_text_measured(
    text: &str,
    max_width: f32,
    font_id: &egui::FontId,
    fonts: &mut egui::epaint::text::FontsView<'_>,
) -> String {
    if max_width <= 0.0 {
        return String::new();
    }

    let full_width = measure_text_width(text, font_id, fonts);
    if full_width <= max_width {
        return text.to_string();
    }

    let text_chars: Vec<char> = text.chars().collect();
    let text_len = text_chars.len();

    if text_len <= 2 {
        return text.to_string();
    }

    let ellipsis = "..";
    let ellipsis_width = measure_text_width(ellipsis, font_id, fonts);

    if ellipsis_width >= max_width {
        return String::new();
    }

    let available_width = max_width - ellipsis_width;
    let mut best_start = 0usize;
    let mut best_end = 0usize;

    for start_chars in (1..=text_len.saturating_sub(1)).rev() {
        let start: String = text_chars[..start_chars].iter().collect();
        let start_width = measure_text_width(&start, font_id, fonts);

        if start_width > available_width {
            continue;
        }

        let remaining_width = available_width - start_width;

        for end_chars in (0..=text_len.saturating_sub(start_chars)).rev() {
            if end_chars == 0 {
                if start_chars > best_start + best_end {
                    best_start = start_chars;
                    best_end = 0;
                }
                break;
            }

            let end: String = text_chars[text_len - end_chars..].iter().collect();
            let end_width = measure_text_width(&end, font_id, fonts);

            if end_width <= remaining_width {
                let total_chars = start_chars + end_chars;
                if total_chars > best_start + best_end {
                    best_start = start_chars;
                    best_end = end_chars;
                }
                break;
            }
        }

        if best_start + best_end >= text_len.saturating_sub(2) {
            break;
        }
    }

    if best_start == 0 && best_end == 0 {
        return ellipsis.to_string();
    }

    let start: String = text_chars[..best_start].iter().collect();
    if best_end == 0 {
        format!("{}{}", start, ellipsis)
    } else {
        let end: String = text_chars[text_len - best_end..].iter().collect();
        format!("{}{}{}", start, ellipsis, end)
    }
}

/// Measures the width of text using egui's font system.
#[inline]
pub(super) fn measure_text_width(
    text: &str,
    font_id: &egui::FontId,
    fonts: &mut egui::epaint::text::FontsView<'_>,
) -> f32 {
    let job = egui::text::LayoutJob::simple_singleline(
        text.to_string(),
        font_id.clone(),
        egui::Color32::WHITE,
    );
    fonts.layout_job(job).rect.width()
}
