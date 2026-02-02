//! Utility functions for rendering UI components

use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

pub fn render_scrollable_list(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem>,
    selected_index: usize,
    block: Block,
) {
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default()); // Highlight handled by item styles

    let mut list_state = ListState::default();
    list_state.select(Some(selected_index));

    frame.render_stateful_widget(list, area, &mut list_state);
}

pub fn format_duration(ms: u32) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02}", minutes, seconds)
}

/// Calculate width needed for index column (log10(n) + padding)
pub fn calculate_num_width(item_count: usize) -> usize {
    if item_count == 0 {
        2
    } else {
        let digits = (item_count as f64).log10().floor() as usize + 1;
        digits + 1
    }
}

pub fn truncate_string(s: &str, max_width: usize) -> String {
    if s.chars().count() > max_width {
        let truncated: String = s.chars().take(max_width.saturating_sub(3)).collect();
        format!("{:<width$}", format!("{}...", truncated), width = max_width)
    } else {
        format!("{:<width$}", s, width = max_width)
    }
}

/// Calculate column widths for track listings
/// Returns (num_width, liked_width, title_width, artist_width, duration_width)
#[allow(dead_code)]
pub fn calculate_track_column_widths(content_width: usize, item_count: usize) -> (usize, usize, usize, usize, usize) {
    // Format: " {num}   {liked}   {title}   {artist}   {duration}"
    // Fixed parts: leading space(1) + num + sep(3) + liked(2) + sep(3) + sep(3) + sep(3) + duration(8)
    let num_width = calculate_num_width(item_count);
    let liked_width = 2;
    let duration_width = 8;
    let fixed_width = 1 + num_width + 3 + liked_width + 3 + 3 + 3 + duration_width;
    let remaining_width = content_width.saturating_sub(fixed_width);
    let title_width = (remaining_width * 55) / 100;
    let artist_width = remaining_width.saturating_sub(title_width);

    (num_width, liked_width, title_width, artist_width, duration_width)
}
