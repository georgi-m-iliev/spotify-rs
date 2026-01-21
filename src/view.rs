use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::model::{ActiveSection, RepeatState, TrackInfo, UiState};

pub struct AppView;

impl AppView {
    pub fn render(frame: &mut Frame, track: &TrackInfo, is_playing: bool, ui_state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar + device
                Constraint::Min(0),    // Main content (sidebar + content)
                Constraint::Length(3), // Progress bar with playback info
            ])
            .split(frame.area());

        // Top bar: Search + Device
        Self::render_top_bar(frame, chunks[0], ui_state);

        // Middle: Sidebar (Library + Playlists) and Main Content
        Self::render_main_area(frame, chunks[1], ui_state);

        // Bottom: Progress bar with track info and controls
        Self::render_progress_bar(frame, chunks[2], track, is_playing, ui_state);
    }

    fn render_top_bar(frame: &mut Frame, area: Rect, ui_state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),        // Search input
                Constraint::Length(25),    // Device name
            ])
            .split(area);

        // Search input
        let search_style = if ui_state.active_section == ActiveSection::Search {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        let search_text = if ui_state.search_query.is_empty() {
            "Type to search..."
        } else {
            &ui_state.search_query
        };

        let search = Paragraph::new(search_text)
            .style(search_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Search ")
                    .border_style(if ui_state.active_section == ActiveSection::Search {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    }),
            );
        frame.render_widget(search, chunks[0]);

        // Device name
        let device = Paragraph::new(format!("🎵 {}", ui_state.device_name))
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).title(" Device "));
        frame.render_widget(device, chunks[1]);
    }

    fn render_main_area(frame: &mut Frame, area: Rect, ui_state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // Sidebar (Library + Playlists)
                Constraint::Percentage(70), // Main content
            ])
            .split(area);

        // Sidebar: Library and Playlists stacked vertically
        Self::render_sidebar(frame, chunks[0], ui_state);

        // Main content area
        Self::render_main_content(frame, chunks[1], ui_state);
    }

    fn render_sidebar(frame: &mut Frame, area: Rect, ui_state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(30), // Library
                Constraint::Percentage(70), // Playlists
            ])
            .split(area);

        // Library section
        let library_items: Vec<ListItem> = ui_state
            .library_items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == ui_state.library_selected
                    && ui_state.active_section == ActiveSection::Library
                {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if i == ui_state.library_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("  {}", item.name)).style(style)
            })
            .collect();

        let library_border_style = if ui_state.active_section == ActiveSection::Library {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let library = List::new(library_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Library ")
                .border_style(library_border_style),
        );
        frame.render_widget(library, chunks[0]);

        // Playlists section
        let playlist_items: Vec<ListItem> = ui_state
            .playlists
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == ui_state.playlist_selected
                    && ui_state.active_section == ActiveSection::Playlists
                {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if i == ui_state.playlist_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("  {}", item.name)).style(style)
            })
            .collect();

        let playlists_border_style = if ui_state.active_section == ActiveSection::Playlists {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let playlists = List::new(playlist_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Playlists ")
                .border_style(playlists_border_style),
        );
        frame.render_widget(playlists, chunks[1]);
    }

    fn render_main_content(frame: &mut Frame, area: Rect, ui_state: &UiState) {
        let border_style = if ui_state.active_section == ActiveSection::MainContent {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        // For now, show placeholder content
        let content = Paragraph::new("Select a playlist or library item to view content\n\nUse Tab to navigate between sections\nUse ↑/↓ to select items\nPress Enter to open")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Content ")
                    .border_style(border_style),
            );
        frame.render_widget(content, area);
    }

    fn render_progress_bar(
        frame: &mut Frame,
        area: Rect,
        track: &TrackInfo,
        is_playing: bool,
        ui_state: &UiState,
    ) {
        // Build the track status text
        let status_text = if track.name == "No track playing" {
            "No track playing".to_string()
        } else if is_playing {
            format!(
                "▶ {} | {} ({})",
                track.name, track.artist, track.album
            )
        } else {
            format!(
                "⏸ {} | {} ({})",
                track.name, track.artist, track.album
            )
        };

        // Shuffle, Repeat, Volume info
        let shuffle_text = if ui_state.shuffle { "Shuffle: On" } else { "Shuffle: Off" };
        let repeat_text = match ui_state.repeat {
            RepeatState::Off => "Repeat: Off",
            RepeatState::All => "Repeat: All",
            RepeatState::One => "Repeat: One",
        };
        let volume_text = format!("Vol: {}%", ui_state.volume);

        // Time info
        let time_str = format!(
            "{} / {}",
            Self::format_duration(track.progress_ms),
            Self::format_duration(track.duration_ms)
        );

        // Calculate progress ratio
        let progress_ratio = if track.duration_ms > 0 {
            (track.progress_ms as f64 / track.duration_ms as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Create a custom layout for the progress bar area
        let inner_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(0)
            .constraints([Constraint::Length(3)])
            .split(area);

        // Build title with track info on left, controls on right
        let title = format!(" {} ", status_text);
        let controls_info = format!("{} | {} | {} ", shuffle_text, repeat_text, volume_text);

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

    fn format_duration(ms: u32) -> String {
        let total_seconds = ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}:{:02}", minutes, seconds)
    }
}
