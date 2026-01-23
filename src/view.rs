use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
};
use ratatui::widgets::Padding;
use crate::model::{
    ActiveSection, ArtistDetailSection, ContentState, ContentView, PlaybackInfo,
    RepeatState, SearchResultSection, UiState,
};

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
        Self::render_top_bar(frame, chunks[0], ui_state, &playback.settings.device_name);

        // Middle: Sidebar (Library + Playlists) and Main Content
        Self::render_main_area(frame, chunks[1], ui_state, content_state);

        // Bottom: Progress bar with track info and controls
        Self::render_progress_bar(frame, chunks[2], playback);

        // Error notification overlay (if there's an error)
        if ui_state.error_message.is_some() {
            Self::render_error_notification(frame, ui_state);
        }
    }

    /// Helper to render a scrollable list with proper state management
    fn render_scrollable_list(
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

    fn render_top_bar(frame: &mut Frame, area: Rect, ui_state: &UiState, device_name: &str) {
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

    fn render_main_area(frame: &mut Frame, area: Rect, ui_state: &UiState, content_state: &ContentState) {
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
        Self::render_main_content(frame, chunks[1], ui_state, content_state);
    }

    fn render_sidebar(frame: &mut Frame, area: Rect, ui_state: &UiState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6), // Library (4 items + 2 border lines)
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

        // Playlists section - use stateful list for scrolling
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

        // Use ListState for scrolling
        let mut list_state = ListState::default();
        list_state.select(Some(ui_state.playlist_selected));

        frame.render_stateful_widget(playlists, chunks[1], &mut list_state);
    }

    fn render_main_content(frame: &mut Frame, area: Rect, ui_state: &UiState, content_state: &ContentState) {
        let is_focused = ui_state.active_section == ActiveSection::MainContent;
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        if content_state.is_loading {
            let loading = Paragraph::new("Loading...")
                .style(Style::default().fg(Color::Yellow))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Content ")
                        .border_style(border_style),
                );
            frame.render_widget(loading, area);
            return;
        }

        match &content_state.view {
            ContentView::Empty => {
                let content = Paragraph::new("Type in search and press Enter to find music\n\nUse Tab to navigate between sections\nUse ↑/↓ to select items\nPress Enter to open")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .padding(Padding::horizontal(1))
                            .border_style(border_style),
                    );
                frame.render_widget(content, area);
            }
            ContentView::SearchResults {
                results,
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
            } => {
                Self::render_search_results(
                    frame,
                    area,
                    results,
                    *section,
                    *track_index,
                    *album_index,
                    *artist_index,
                    *playlist_index,
                    is_focused,
                );
            }
            ContentView::AlbumDetail { detail, selected_index } => {
                Self::render_album_detail(frame, area, detail, *selected_index, is_focused);
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                Self::render_playlist_detail(frame, area, detail, *selected_index, is_focused);
            }
            ContentView::ArtistDetail {
                detail,
                section,
                track_index,
                album_index,
            } => {
                Self::render_artist_detail(
                    frame,
                    area,
                    detail,
                    *section,
                    *track_index,
                    *album_index,
                    is_focused,
                );
            }
            ContentView::LikedSongs { tracks, selected_index } => {
                Self::render_track_list(
                    frame,
                    area,
                    " Liked Songs ",
                    tracks,
                    *selected_index,
                    is_focused,
                );
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                Self::render_track_list(
                    frame,
                    area,
                    " Recently Played ",
                    tracks,
                    *selected_index,
                    is_focused,
                );
            }
            ContentView::SavedAlbums { albums, selected_index } => {
                Self::render_album_list(
                    frame,
                    area,
                    " Your Albums ",
                    albums,
                    *selected_index,
                    is_focused,
                );
            }
            ContentView::FollowedArtists { artists, selected_index } => {
                Self::render_artist_list(
                    frame,
                    area,
                    " Followed Artists ",
                    artists,
                    *selected_index,
                    is_focused,
                );
            }
        }
    }

    fn render_search_results(
        frame: &mut Frame,
        area: Rect,
        results: &crate::model::SearchResults,
        section: SearchResultSection,
        track_index: usize,
        album_index: usize,
        artist_index: usize,
        playlist_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        // Split into tabs area and content area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Category tabs
                Constraint::Min(0),    // Results list
            ])
            .split(area);

        // Render category tabs
        let tab_titles = vec![
            format!(" Songs ({}) ", results.tracks.len()),
            format!(" Albums ({}) ", results.albums.len()),
            format!(" Artists ({}) ", results.artists.len()),
            format!(" Playlists ({}) ", results.playlists.len()),
        ];

        let tabs_content: Vec<ratatui::text::Span> = tab_titles
            .iter()
            .enumerate()
            .flat_map(|(i, title)| {
                let tab_section = match i {
                    0 => SearchResultSection::Tracks,
                    1 => SearchResultSection::Albums,
                    2 => SearchResultSection::Artists,
                    _ => SearchResultSection::Playlists,
                };
                let style = if tab_section == section {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                vec![
                    ratatui::text::Span::styled(title.clone(), style),
                    ratatui::text::Span::raw("  "),
                ]
            })
            .collect();

        let tabs_line = ratatui::text::Line::from(tabs_content);
        let tabs = Paragraph::new(tabs_line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Results (←/→ to switch) ")
                    .border_style(border_style),
            );
        frame.render_widget(tabs, chunks[0]);

        // Render the selected category's results
        let list_items: Vec<ListItem> = match section {
            SearchResultSection::Tracks => {
                results.tracks.iter().enumerate().map(|(i, track)| {
                    let duration = Self::format_duration(track.duration_ms);
                    let style = if i == track_index && is_focused {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if i == track_index {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("{} - {} [{}]", track.name, track.artist, duration)).style(style)
                }).collect()
            }
            SearchResultSection::Albums => {
                results.albums.iter().enumerate().map(|(i, album)| {
                    let style = if i == album_index && is_focused {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if i == album_index {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("{} - {} ({})", album.name, album.artist, album.year)).style(style)
                }).collect()
            }
            SearchResultSection::Artists => {
                results.artists.iter().enumerate().map(|(i, artist)| {
                    let style = if i == artist_index && is_focused {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if i == artist_index {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    let genres = if artist.genres.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", artist.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "))
                    };
                    ListItem::new(format!("{}{}", artist.name, genres)).style(style)
                }).collect()
            }
            SearchResultSection::Playlists => {
                results.playlists.iter().enumerate().map(|(i, playlist)| {
                    let style = if i == playlist_index && is_focused {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if i == playlist_index {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("{} by {} ({} tracks)", playlist.name, playlist.owner, playlist.total_tracks)).style(style)
                }).collect()
            }
        };

        let empty_msg = match section {
            SearchResultSection::Tracks => "No songs found",
            SearchResultSection::Albums => "No albums found",
            SearchResultSection::Artists => "No artists found",
            SearchResultSection::Playlists => "No playlists found",
        };

        if list_items.is_empty() {
            let empty = Paragraph::new(format!("  {}", empty_msg))
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .padding(Padding::horizontal(1))
                        .border_style(border_style),
                );
            frame.render_widget(empty, chunks[1]);
        } else {
            // Determine selected index based on section
            let selected_index = match section {
                SearchResultSection::Tracks => track_index,
                SearchResultSection::Albums => album_index,
                SearchResultSection::Artists => artist_index,
                SearchResultSection::Playlists => playlist_index,
            };

            let list_block = Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1))
                .border_style(border_style);

            Self::render_scrollable_list(frame, chunks[1], list_items, selected_index, list_block);
        }
    }

    fn render_album_detail(
        frame: &mut Frame,
        area: Rect,
        detail: &crate::model::AlbumDetail,
        selected_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Header
                Constraint::Min(0),    // Tracks
            ])
            .split(area);

        // Header
        let header_text = format!(
            " 💿 {} by {} ({})\n {} tracks | Enter: Play from selected | Backspace: Go back",
            detail.name,
            detail.artist,
            detail.year,
            detail.tracks.len()
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).border_style(border_style));
        frame.render_widget(header, chunks[0]);

        // Tracks - use scrollable list
        let track_items: Vec<ListItem> = detail
            .tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let duration = Self::format_duration(track.duration_ms);
                let style = if i == selected_index && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == selected_index {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}. {} [{}]", i + 1, track.name, duration)).style(style)
            })
            .collect();

        let tracks_block = Block::default()
            .borders(Borders::ALL)
            .title(" Tracks ")
            .padding(Padding::horizontal(1))
            .border_style(border_style);

        Self::render_scrollable_list(frame, chunks[1], track_items, selected_index, tracks_block);
    }

    fn render_playlist_detail(
        frame: &mut Frame,
        area: Rect,
        detail: &crate::model::PlaylistDetail,
        selected_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Header
                Constraint::Min(0),    // Tracks
            ])
            .split(area);

        // Header
        let header_text = format!(
            " 📻 {} by {}\n {} tracks | Enter: Play from selected | Backspace: Go back",
            detail.name,
            detail.owner,
            detail.tracks.len()
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).border_style(border_style));
        frame.render_widget(header, chunks[0]);

        // Tracks - use scrollable list
        let track_items: Vec<ListItem> = detail
            .tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let duration = Self::format_duration(track.duration_ms);
                let style = if i == selected_index && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == selected_index {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} - {} [{}]", track.name, track.artist, duration)).style(style)
            })
            .collect();

        let tracks_block = Block::default()
            .borders(Borders::ALL)
            .title(" Tracks ")
            .padding(Padding::horizontal(1))
            .border_style(border_style);

        Self::render_scrollable_list(frame, chunks[1], track_items, selected_index, tracks_block);
    }

    fn render_artist_detail(
        frame: &mut Frame,
        area: Rect,
        detail: &crate::model::ArtistDetail,
        section: ArtistDetailSection,
        track_index: usize,
        album_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Content (top tracks + albums)
            ])
            .split(area);

        // Header
        let genres = if detail.genres.is_empty() {
            String::new()
        } else {
            format!(" | {}", detail.genres.join(", "))
        };
        let header_text = format!(
            " {}{} | Press ←/→ to switch sections, Backspace to go back",
            detail.name, genres
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).border_style(border_style));
        frame.render_widget(header, chunks[0]);

        // Content: Top tracks and Albums side by side
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[1]);

        // Top tracks
        let track_items: Vec<ListItem> = detail
            .top_tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let duration = Self::format_duration(track.duration_ms);
                let style = if i == track_index && section == ArtistDetailSection::TopTracks && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == track_index && section == ArtistDetailSection::TopTracks {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}. {} [{}]", i + 1, track.name, duration)).style(style)
            })
            .collect();

        let tracks_border = if section == ArtistDetailSection::TopTracks && is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let tracks_block = Block::default()
            .borders(Borders::ALL)
            .title(" Top Tracks ")
            .padding(Padding::horizontal(1))
            .border_style(tracks_border);

        Self::render_scrollable_list(frame, content_chunks[0], track_items, track_index, tracks_block);

        // Albums
        let album_items: Vec<ListItem> = detail
            .albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let style = if i == album_index && section == ArtistDetailSection::Albums && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == album_index && section == ArtistDetailSection::Albums {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} ({})", album.name, album.year)).style(style)
            })
            .collect();

        let albums_border = if section == ArtistDetailSection::Albums && is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let albums_block = Block::default()
            .borders(Borders::ALL)
            .title(" Albums ")
            .padding(Padding::horizontal(1))
            .border_style(albums_border);

        Self::render_scrollable_list(frame, content_chunks[1], album_items, album_index, albums_block);
    }

    fn render_progress_bar(
        frame: &mut Frame,
        area: Rect,
        playback: &PlaybackInfo,
    ) {
        // Build the track status text
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

        // Shuffle, Repeat, Volume info
        let shuffle_text = if playback.settings.shuffle { "Shuffle: On" } else { "Shuffle: Off" };
        let repeat_text = match playback.settings.repeat {
            RepeatState::Off => "Repeat: Off",
            RepeatState::All => "Repeat: All",
            RepeatState::One => "Repeat: One",
        };
        let volume_text = format!("Vol: {}%", playback.settings.volume);

        // Time info
        let time_str = format!(
            "{} / {}",
            Self::format_duration(playback.progress_ms),
            Self::format_duration(playback.duration_ms)
        );

        // Calculate progress ratio
        let progress_ratio = if playback.duration_ms > 0 {
            (playback.progress_ms as f64 / playback.duration_ms as f64).clamp(0.0, 1.0)
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

    fn format_duration(ms: u32) -> String {
        let total_seconds = ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}:{:02}", minutes, seconds)
    }

    fn render_error_notification(frame: &mut Frame, ui_state: &UiState) {
        if let Some(ref error_msg) = ui_state.error_message {
            let area = frame.area();

            // Calculate centered popup size
            let popup_width = error_msg.len().min(60_usize) as u16 + 4;
            let popup_height = 5;

            let popup_x = area.width.saturating_sub(popup_width) / 2;
            let popup_y = area.height.saturating_sub(popup_height) / 2;

            let popup_area = Rect {
                x: popup_x,
                y: popup_y,
                width: popup_width,
                height: popup_height,
            };

            // Create error popup
            let error_text = format!("⚠ {}", error_msg);
            let error_widget = Paragraph::new(error_text)
                .style(
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Red))
                        .title(" Error ")
                        .style(Style::default().bg(Color::Black)),
                );

            frame.render_widget(error_widget, popup_area);
        }
    }

    /// Render a list of tracks (for Liked Songs, Recently Played)
    fn render_track_list(
        frame: &mut Frame,
        area: Rect,
        title: &str,
        tracks: &[crate::model::SearchTrack],
        selected_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let list_items: Vec<ListItem> = tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let duration = Self::format_duration(track.duration_ms);
                let style = if i == selected_index && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == selected_index {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} - {} [{}]", track.name, track.artist, duration)).style(style)
            })
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .padding(Padding::horizontal(1))
                    .border_style(border_style),
            )
            .highlight_style(Style::default());

        let mut list_state = ListState::default();
        list_state.select(Some(selected_index));

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    /// Render a list of albums (for Saved Albums)
    fn render_album_list(
        frame: &mut Frame,
        area: Rect,
        title: &str,
        albums: &[crate::model::SearchAlbum],
        selected_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let list_items: Vec<ListItem> = albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let style = if i == selected_index && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == selected_index {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{} - {} ({})", album.name, album.artist, album.year)).style(style)
            })
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .padding(Padding::horizontal(1))
                    .border_style(border_style),
            )
            .highlight_style(Style::default());

        let mut list_state = ListState::default();
        list_state.select(Some(selected_index));

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    /// Render a list of artists (for Followed Artists)
    fn render_artist_list(
        frame: &mut Frame,
        area: Rect,
        title: &str,
        artists: &[crate::model::SearchArtist],
        selected_index: usize,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let list_items: Vec<ListItem> = artists
            .iter()
            .enumerate()
            .map(|(i, artist)| {
                let style = if i == selected_index && is_focused {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if i == selected_index {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let genres = if artist.genres.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", artist.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "))
                };
                ListItem::new(format!("{}{}", artist.name, genres)).style(style)
            })
            .collect();

        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .padding(Padding::horizontal(1))
                    .border_style(border_style),
            )
            .highlight_style(Style::default());

        let mut list_state = ListState::default();
        list_state.select(Some(selected_index));

        frame.render_stateful_widget(list, area, &mut list_state);
    }
}
