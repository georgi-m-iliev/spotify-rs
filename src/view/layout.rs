//! Layout rendering (top bar, sidebar, main area structure)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use ratatui::widgets::Padding;

use crate::model::{ActiveSection, UiState};

pub fn render_top_bar(frame: &mut Frame, area: Rect, ui_state: &UiState, device_name: &str) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),        // Search input
            Constraint::Length(25),    // Device name
        ])
        .split(area);

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
                .padding(Padding::horizontal(1))
                .border_style(if ui_state.active_section == ActiveSection::Search {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                }),
        );
    frame.render_widget(search, chunks[0]);

    // Device name
    let device = Paragraph::new(format!("🎵 {}", device_name))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title(" Device "));
    frame.render_widget(device, chunks[1]);
}

pub fn render_sidebar(frame: &mut Frame, area: Rect, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Library (4 items + 2 borderlines)
            Constraint::Min(0),    // Playlists (fills remaining space)
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
            ListItem::new(format!("{}", item.name)).style(style)
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
            .padding(Padding::horizontal(1))
            .border_style(library_border_style),
    );
    frame.render_widget(library, chunks[0]);

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
            ListItem::new(format!("{}", item.name)).style(style)
        })
        .collect();

    let playlists_border_style = if ui_state.active_section == ActiveSection::Playlists {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let playlists = List::new(playlist_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Playlists ")
                .padding(Padding::horizontal(1))
                .border_style(playlists_border_style),
        )
        .highlight_style(Style::default()); // Highlight handled by item styles

    let mut list_state = ListState::default();
    list_state.select(Some(ui_state.playlist_selected));

    frame.render_stateful_widget(playlists, chunks[1], &mut list_state);
}
