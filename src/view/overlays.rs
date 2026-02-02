//! Overlay rendering (error notification, device picker, help popup)

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::model::UiState;

pub fn render_error_notification(frame: &mut Frame, ui_state: &UiState) {
    if let Some(ref error_msg) = ui_state.error_message {
        let area = frame.area();

        // Fixed width popup (responsive to screen size)
        let popup_width = 52.min(area.width.saturating_sub(4));
        let inner_width = popup_width.saturating_sub(4) as usize; // account for borders

        // Calculate how many lines the error message will take when wrapped
        let error_line_count = ((error_msg.chars().count() as f32) / (inner_width as f32)).ceil() as u16;

        // Height: top border (1) + error lines + bottom border (1)
        let popup_height = (2 + error_line_count.max(1)).min(area.height - 4);

        let popup_x = area.width.saturating_sub(popup_width) / 2;
        let popup_y = area.height.saturating_sub(popup_height) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup first
        frame.render_widget(Clear, popup_area);

        // Create text with error message and dismiss hint
        let error_widget = Paragraph::new(error_msg.to_string())
            .style(Style::default().fg(Color::Red))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" Error (Esc to dismiss) ")
                    .title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                    .style(Style::default().bg(Color::Black)),
            );

        frame.render_widget(error_widget, popup_area);
    }
}

pub fn render_device_picker(frame: &mut Frame, ui_state: &UiState) {
    let area = frame.area();

    // Calculate popup size based on number of devices
    let device_count = ui_state.available_devices.len();
    let max_name_len = ui_state
        .available_devices
        .iter()
        .map(|d| d.name.len() + 6) // icon + name + spacing
        .max()
        .unwrap_or(30);

    let popup_width = (max_name_len as u16 + 6).min(60).max(35);
    let popup_height = (device_count as u16 + 4).min(area.height - 4).max(6);

    let popup_x = area.width.saturating_sub(popup_width) / 2;
    let popup_y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the area behind the popup first
    frame.render_widget(Clear, popup_area);

    // Create device list items
    let items: Vec<ListItem> = ui_state
        .available_devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let is_selected = i == ui_state.device_selected;
            let is_active = device.is_active;

            // Active indicator
            let active_indicator = if is_active { " ●" } else { "" };

            let text = format!("🎵 {}{}", device.name, active_indicator);

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Select Device (↑↓ Enter Esc) ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(Color::Black)),
    );

    let mut list_state = ListState::default();
    list_state.select(Some(ui_state.device_selected));

    frame.render_stateful_widget(list, popup_area, &mut list_state);
}

pub fn render_help_popup(frame: &mut Frame) {
    let area = frame.area();

    // Define keybindings organized by category
    let keybindings = vec![
        ("", "── Navigation ──"),
        ("Tab / Shift+Tab", "Cycle sections"),
        ("↑ / ↓", "Move selection"),
        ("← / →", "Switch search category"),
        ("Enter", "Select / Play"),
        ("Backspace / Esc", "Go back"),
        ("G", "Focus search"),
        ("L", "Focus playlists"),
        ("", ""),
        ("", "── Playback ──"),
        ("Space", "Play / Pause"),
        ("N", "Next track"),
        ("P", "Previous track"),
        ("S", "Toggle shuffle"),
        ("R", "Cycle repeat (off → all → one)"),
        ("+ / -", "Volume up / down"),
        ("", ""),
        ("", "── Actions ──"),
        ("X", "Like / Unlike track"),
        ("K", "Add to queue"),
        ("Delete", "Remove from queue"),
        ("U", "Show queue"),
        ("D", "Device picker"),
        ("", ""),
        ("", "── General ──"),
        ("H", "Toggle this help"),
        ("Q", "Quit"),
    ];

    let popup_width = 62;
    let popup_height = (keybindings.len() as u16 + 2).min(area.height - 4);

    let popup_x = area.width.saturating_sub(popup_width) / 2;
    let popup_y = area.height.saturating_sub(popup_height) / 2;

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Create help text lines
    let lines: Vec<Line> = keybindings
        .iter()
        .map(|(key, desc)| {
            if key.is_empty() {
                // Section header or empty line
                Line::from(Span::styled(
                    format!("{:^38}", desc),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{:>18}", key),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        desc.to_string(),
                        Style::default().fg(Color::White),
                    ),
                ])
            }
        })
        .collect();

    let help_text = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help (H or Esc to close) ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .style(Style::default().bg(Color::Black)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(help_text, popup_area);
}
