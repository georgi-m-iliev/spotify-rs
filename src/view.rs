use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::model::TrackInfo;

pub struct AppView;

impl AppView {
    pub fn render(frame: &mut Frame, track: &TrackInfo, is_playing: bool) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main content
                Constraint::Length(3), // Progress bar
                Constraint::Length(5), // Controls
            ])
            .split(frame.area());

        // Header
        Self::render_header(frame, chunks[0]);

        // Main content - Track info
        Self::render_track_info(frame, chunks[1], track, is_playing);

        // Progress bar
        Self::render_progress(frame, chunks[2], track);

        // Controls
        Self::render_controls(frame, chunks[3]);
    }

    fn render_header(frame: &mut Frame, area: Rect) {
        let header = Paragraph::new("🎵 Spotify-RS - Rust Spotify Client")
            .style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(header, area);
    }

    fn render_track_info(frame: &mut Frame, area: Rect, track: &TrackInfo, is_playing: bool) {
        let status = if is_playing {
            "▶ Playing"
        } else {
            "⏸ Paused"
        };

        let track_info = if track.name == "No track playing" {
            format!("{}\n\n{}", status, track.name)
        } else {
            format!(
                "{}\n\nTrack: {}\nArtist: {}\nAlbum: {}",
                status, track.name, track.artist, track.album
            )
        };

        let content = Paragraph::new(track_info)
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Now Playing"));
        frame.render_widget(content, area);
    }

    fn render_progress(frame: &mut Frame, area: Rect, track: &TrackInfo) {
        let progress = if track.duration_ms > 0 {
            (track.progress_ms as f64 / track.duration_ms as f64) * 100.0
        } else {
            0.0
        };

        let time_str = format!(
            "{} / {}",
            Self::format_duration(track.progress_ms),
            Self::format_duration(track.duration_ms)
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(progress as u16)
            .label(time_str);

        frame.render_widget(gauge, area);
    }

    fn render_controls(frame: &mut Frame, area: Rect) {
        let controls = Paragraph::new(
            "Controls:\n[Space] Play/Pause | [N] Next | [P] Previous | [Q] Quit\n[R] Refresh playback state",
        )
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Controls"));
        frame.render_widget(controls, area);
    }

    fn format_duration(ms: u32) -> String {
        let total_seconds = ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}:{:02}", minutes, seconds)
    }
}
