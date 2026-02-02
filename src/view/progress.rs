//! Progress bar rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Gauge},
    Frame,
};

use crate::model::{PlaybackInfo, RepeatState};
use super::utils::format_duration;

pub fn render_progress_bar(
    frame: &mut Frame,
    area: Rect,
    playback: &PlaybackInfo,
) {
    let status_text = if playback.track.name == "No track playing" {
        " No track playing".to_string()
    } else if playback.is_playing {
        format!(
            " ▶ {} | {} ({})",
            playback.track.name, playback.track.artist, playback.track.album
        )
    } else {
        format!(
            "⏸  {} | {} ({})",
            playback.track.name, playback.track.artist, playback.track.album
        )
    };

    let shuffle_text = if playback.settings.shuffle { "Shuffle: On" } else { "Shuffle: Off" };
    let repeat_text = match playback.settings.repeat {
        RepeatState::Off => "Repeat: Off",
        RepeatState::All => "Repeat: All",
        RepeatState::One => "Repeat: One",
    };
    let volume_text = format!("Vol: {}%", playback.settings.volume);

    let time_str = format!(
        "{} / {}",
        format_duration(playback.progress_ms),
        format_duration(playback.duration_ms)
    );

    let progress_ratio = if playback.duration_ms > 0 {
        (playback.progress_ms as f64 / playback.duration_ms as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([Constraint::Length(3)])
        .split(area);

    let title = format!("{} ", status_text);
    let controls_info = format!(" {} | {} | {} ", shuffle_text, repeat_text, volume_text);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(Line::from(controls_info).right_aligned()),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(progress_ratio)
        .label(time_str);

    frame.render_widget(gauge, inner_chunks[0]);
}
