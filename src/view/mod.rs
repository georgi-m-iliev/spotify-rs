//! View module - UI rendering
//!
//! This module handles all UI rendering for the application using ratatui.
//! It is organized into submodules by component type:
//!
//! - `utils`: Shared utility functions (formatting, scrollable lists)
//! - `layout`: Main layout structure (top bar, sidebar)
//! - `content`: Main content area rendering
//! - `progress`: Progress bar rendering
//! - `overlays`: Modal overlays (error, device picker, help)

mod utils;
mod layout;
mod content;
mod progress;
mod overlays;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::model::{ContentState, PlaybackInfo, UiState};

pub struct AppView;

impl AppView {
    pub fn render(frame: &mut Frame, playback: &PlaybackInfo, ui_state: &UiState, content_state: &ContentState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar + device
                Constraint::Min(0),    // Main content (sidebar + content)
                Constraint::Length(3), // Progress bar with playback info
            ])
            .split(frame.area());

        // Top bar: Search + Device
        layout::render_top_bar(frame, chunks[0], ui_state, &playback.settings.device_name);

        // Middle: Sidebar (Library + Playlists) and Main Content
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // Sidebar (Library + Playlists)
                Constraint::Percentage(70), // Main content
            ])
            .split(chunks[1]);

        // Sidebar: Library and Playlists stacked vertically
        layout::render_sidebar(frame, main_chunks[0], ui_state);

        // Main content area
        let current_playing_uri = if !playback.track.uri.is_empty() {
            Some(playback.track.uri.as_str())
        } else {
            None
        };
        content::render_main_content(frame, main_chunks[1], ui_state, content_state, current_playing_uri);

        // Bottom: Progress bar with track info and controls
        progress::render_progress_bar(frame, chunks[2], playback);

        // Error notification overlay (if there's an error)
        if ui_state.error_message.is_some() {
            overlays::render_error_notification(frame, ui_state);
        }

        // Device picker overlay (if open)
        if ui_state.show_device_picker {
            overlays::render_device_picker(frame, ui_state);
        }

        // Help popup overlay (if open)
        if ui_state.show_help_popup {
            overlays::render_help_popup(frame);
        }
    }
}
