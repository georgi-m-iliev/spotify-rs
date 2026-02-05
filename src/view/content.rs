//! Main content area rendering (search results, detail views, lists)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use ratatui::widgets::Padding;

use crate::model::{
    ActiveSection, ArtistDetailSection, ContentState, ContentView,
    SearchResultSection, UiState, AlbumDetail, PlaylistDetail, ArtistDetail,
    SearchResults, SearchTrack, SearchAlbum, SearchArtist,
};
use super::utils::{calculate_num_width, format_duration, render_scrollable_list, truncate_string};

pub fn render_main_content(
    frame: &mut Frame,
    area: Rect,
    ui_state: &UiState,
    content_state: &ContentState,
    current_playing_uri: Option<&str>,
) {
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
            render_search_results(
                frame,
                area,
                results,
                *section,
                *track_index,
                *album_index,
                *artist_index,
                *playlist_index,
                is_focused,
                current_playing_uri,
            );
        }
        ContentView::AlbumDetail { detail, selected_index } => {
            render_album_detail(frame, area, detail, *selected_index, is_focused, current_playing_uri);
        }
        ContentView::PlaylistDetail { detail, selected_index } => {
            render_playlist_detail(frame, area, detail, *selected_index, is_focused, current_playing_uri);
        }
        ContentView::ArtistDetail {
            detail,
            section,
            track_index,
            album_index,
        } => {
            render_artist_detail(
                frame,
                area,
                detail,
                *section,
                *track_index,
                *album_index,
                is_focused,
                current_playing_uri,
            );
        }
        ContentView::LikedSongs { tracks, selected_index } => {
            render_track_list(
                frame,
                area,
                " Liked Songs ",
                tracks,
                *selected_index,
                is_focused,
                current_playing_uri,
            );
        }
        ContentView::RecentlyPlayed { tracks, selected_index } => {
            render_track_list(
                frame,
                area,
                " Recently Played ",
                tracks,
                *selected_index,
                is_focused,
                current_playing_uri,
            );
        }
        ContentView::SavedAlbums { albums, selected_index } => {
            render_album_list(
                frame,
                area,
                " Your Albums ",
                albums,
                *selected_index,
                is_focused,
            );
        }
        ContentView::FollowedArtists { artists, selected_index } => {
            render_artist_list(
                frame,
                area,
                " Followed Artists ",
                artists,
                *selected_index,
                is_focused,
            );
        }
        ContentView::Queue { currently_playing, queue, selected_index } => {
            render_queue(
                frame,
                area,
                currently_playing.as_ref(),
                queue,
                *selected_index,
                is_focused,
                current_playing_uri,
            );
        }
    }
}

fn render_search_results(
    frame: &mut Frame,
    area: Rect,
    results: &SearchResults,
    section: SearchResultSection,
    track_index: usize,
    album_index: usize,
    artist_index: usize,
    playlist_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
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

    let tab_titles = vec![
        format!(" Songs ({}) ", results.tracks.len()),
        format!(" Albums ({}) ", results.albums.len()),
        format!(" Artists ({}) ", results.artists.len()),
        format!(" Playlists ({}) ", results.playlists.len()),
    ];

    let tabs_content: Vec<Span> = tab_titles
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
                Span::styled(title.clone(), style),
                Span::raw("  "),
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

    let content_width = chunks[1].width.saturating_sub(4) as usize;

    let list_items: Vec<ListItem> = match section {
        SearchResultSection::Tracks => {
            render_track_items(results, track_index, is_focused, current_playing_uri, content_width)
        }
        SearchResultSection::Albums => {
            render_album_items(&results.albums, album_index, is_focused, content_width)
        }
        SearchResultSection::Artists => {
            render_artist_items(&results.artists, artist_index, is_focused, content_width)
        }
        SearchResultSection::Playlists => {
            render_playlist_items(&results.playlists, playlist_index, is_focused, content_width)
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
        let selected_index = match section {
            SearchResultSection::Tracks => track_index + 1, // +1 for header row
            SearchResultSection::Albums => album_index,
            SearchResultSection::Artists => artist_index,
            SearchResultSection::Playlists => playlist_index,
        };

        let list_block = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::horizontal(1))
            .border_style(border_style);

        render_scrollable_list(frame, chunks[1], list_items, selected_index, list_block);
    }
}

fn render_track_items(
    results: &SearchResults,
    track_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
    content_width: usize,
) -> Vec<ListItem<'static>> {
    let num_width = calculate_num_width(results.tracks.len());
    let liked_width = 2;
    let duration_width = 8;
    let fixed_width = 1 + num_width + 3 + liked_width + 3 + 3 + 3 + duration_width;
    let remaining_width = content_width.saturating_sub(fixed_width);
    let title_width = (remaining_width * 55) / 100;
    let artist_width = remaining_width.saturating_sub(title_width);

    // Create header as first item
    let mut items = vec![
        ListItem::new(format!(
            " {:<num_width$}   {}   {:<title_width$}   {:<artist_width$}   {}",
            "#", "  ", "Title", "Artist", "Duration",
            num_width = num_width,
            title_width = title_width,
            artist_width = artist_width
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let track_items: Vec<ListItem> = results.tracks.iter().enumerate().map(|(i, track)| {
        let duration = format_duration(track.duration_ms);
        let is_playing = current_playing_uri.map_or(false, |uri| uri == track.uri);
        let style = if i == track_index && is_focused {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if is_playing {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if i == track_index {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let liked_indicator = if track.liked { "💚" } else { "  " };
        let playing_indicator = if is_playing { "▶" } else { " " };
        let track_num = format!("{}{:<num_width$}", playing_indicator, i + 1, num_width = num_width);

        let title_str = truncate_string(&track.name, title_width);
        let artists_display = if track.artists.len() > 1 {
            track.artists.join(", ")
        } else {
            track.artist.clone()
        };
        let artist_str = truncate_string(&artists_display, artist_width);

        ListItem::new(format!("{}   {}   {}   {}   {}", track_num, liked_indicator, title_str, artist_str, duration)).style(style)
    }).collect();

    items.extend(track_items);
    items
}

fn render_album_items(
    albums: &[SearchAlbum],
    album_index: usize,
    is_focused: bool,
    content_width: usize,
) -> Vec<ListItem<'static>> {
    let album_num_width = calculate_num_width(albums.len());
    let year_width = 4;
    let album_fixed_width = 1 + album_num_width + 3 + 3 + 3 + year_width;
    let album_remaining = content_width.saturating_sub(album_fixed_width);
    let album_name_width = (album_remaining * 50) / 100;
    let album_artist_width = album_remaining.saturating_sub(album_name_width);

    let mut items = vec![
        ListItem::new(format!(
            " {:<num_w$}   {:<album_w$}   {:<artist_w$}   {:>year_w$}",
            "#", "Album", "Artist", "Year",
            num_w = album_num_width,
            album_w = album_name_width,
            artist_w = album_artist_width,
            year_w = year_width
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let album_items: Vec<ListItem> = albums.iter().enumerate().map(|(i, album)| {
        let style = if i == album_index && is_focused {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if i == album_index {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let album_str = truncate_string(&album.name, album_name_width);
        let artist_str = truncate_string(&album.artist, album_artist_width);

        ListItem::new(format!(
            " {:<num_w$}   {}   {}   {:>year_w$}",
            i + 1, album_str, artist_str, album.year,
            num_w = album_num_width,
            year_w = year_width
        )).style(style)
    }).collect();

    items.extend(album_items);
    items
}

fn render_artist_items(
    artists: &[SearchArtist],
    artist_index: usize,
    is_focused: bool,
    content_width: usize,
) -> Vec<ListItem<'static>> {
    let artist_num_width = calculate_num_width(artists.len());
    let artist_fixed_width = 1 + artist_num_width + 3 + 3;
    let artist_remaining = content_width.saturating_sub(artist_fixed_width);
    let artist_name_width = (artist_remaining * 35) / 100;
    let genres_width = artist_remaining.saturating_sub(artist_name_width);

    let mut items = vec![
        ListItem::new(format!(
            " {:<num_w$}   {:<name_w$}   {:<genres_w$}",
            "#", "Artist", "Genres",
            num_w = artist_num_width,
            name_w = artist_name_width,
            genres_w = genres_width
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let artist_items: Vec<ListItem> = artists.iter().enumerate().map(|(i, artist)| {
        let style = if i == artist_index && is_focused {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if i == artist_index {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let name_str = truncate_string(&artist.name, artist_name_width);

        let genres_str = if artist.genres.is_empty() {
            format!("{:<width$}", "-", width = genres_width)
        } else {
            let genres_text = artist.genres.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            truncate_string(&genres_text, genres_width)
        };

        ListItem::new(format!(
            " {:<num_w$}   {}   {}",
            i + 1, name_str, genres_str,
            num_w = artist_num_width
        )).style(style)
    }).collect();

    items.extend(artist_items);
    items
}

fn render_playlist_items(
    playlists: &[crate::model::SearchPlaylist],
    playlist_index: usize,
    is_focused: bool,
    content_width: usize,
) -> Vec<ListItem<'static>> {
    let pl_num_width = calculate_num_width(playlists.len());
    let tracks_width = 8;
    let owner_width = 20;
    let pl_fixed_width = 1 + pl_num_width + 3 + 3 + owner_width + 3 + tracks_width;
    let pl_name_width = content_width.saturating_sub(pl_fixed_width);

    let mut items = vec![
        ListItem::new(format!(
            " {:<num_w$}   {:<name_w$}   {:<owner_w$}   {:>tracks_w$}",
            "#", "Playlist", "Owner", "Tracks",
            num_w = pl_num_width,
            name_w = pl_name_width,
            owner_w = owner_width,
            tracks_w = tracks_width
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let playlist_items: Vec<ListItem> = playlists.iter().enumerate().map(|(i, playlist)| {
        let style = if i == playlist_index && is_focused {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else if i == playlist_index {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let name_str = truncate_string(&playlist.name, pl_name_width);
        let owner_str = truncate_string(&playlist.owner, owner_width);

        ListItem::new(format!(
            " {:<num_w$}   {}   {}   {:>tracks_w$}",
            i + 1, name_str, owner_str, playlist.total_tracks,
            num_w = pl_num_width,
            tracks_w = tracks_width
        )).style(style)
    }).collect();

    items.extend(playlist_items);
    items
}

fn render_album_detail(
    frame: &mut Frame,
    area: Rect,
    detail: &AlbumDetail,
    selected_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
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

    let header_text = format!(
        "💿 {} by {} ({})\n {} tracks | Enter: Play from selected | Backspace: Go back",
        detail.name,
        detail.artist,
        detail.year,
        detail.tracks.len()
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL)
        .border_style(border_style));
    frame.render_widget(header, chunks[0]);

    let content_width = chunks[1].width.saturating_sub(4) as usize;
    let track_items = render_detail_track_items(&detail.tracks, selected_index, is_focused, current_playing_uri, content_width, detail.tracks.len());

    let tracks_block = Block::default()
        .borders(Borders::ALL)
        .title(" Tracks ")
        .padding(Padding::horizontal(1))
        .border_style(border_style);

    render_scrollable_list(frame, chunks[1], track_items, selected_index + 1, tracks_block);
}

fn render_playlist_detail(
    frame: &mut Frame,
    area: Rect,
    detail: &PlaylistDetail,
    selected_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
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

    let loading_indicator = if detail.loading_more { " (loading...)" } else { "" };
    let header_text = format!(
        "📻 {} by {}\n {} tracks{} | Enter: Play from selected | Backspace: Go back",
        detail.name,
        detail.owner,
        detail.total_tracks,
        loading_indicator
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL)
        .border_style(border_style));
    frame.render_widget(header, chunks[0]);

    let content_width = chunks[1].width.saturating_sub(4) as usize;
    let mut track_items = render_detail_track_items(&detail.tracks, selected_index, is_focused, current_playing_uri, content_width, detail.total_tracks as usize);

    // Add loading indicator at the bottom if more tracks are available or loading
    if detail.has_more || detail.loading_more {
        let loading_text = if detail.loading_more {
            "⏳ Loading more tracks..."
        } else {
            "↓ Scroll down for more tracks..."
        };
        track_items.push(
            ListItem::new(format!("       {}", loading_text))
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC))
        );
    }

    let tracks_block = Block::default()
        .borders(Borders::ALL)
        .title(" Tracks ")
        .padding(Padding::horizontal(1))
        .border_style(border_style);

    render_scrollable_list(frame, chunks[1], track_items, selected_index + 1, tracks_block);
}

fn render_artist_detail(
    frame: &mut Frame,
    area: Rect,
    detail: &ArtistDetail,
    section: ArtistDetailSection,
    track_index: usize,
    album_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
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

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[1]);

    let track_content_width = content_chunks[0].width.saturating_sub(4) as usize;
    let track_items = render_artist_track_items(&detail.top_tracks, track_index, section == ArtistDetailSection::TopTracks && is_focused, current_playing_uri, track_content_width);

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

    render_scrollable_list(frame, content_chunks[0], track_items, track_index + 1, tracks_block);

    let album_items: Vec<ListItem> = detail
        .albums
        .iter()
        .enumerate()
        .map(|(i, album)| {
            let style = if i == album_index && section == ArtistDetailSection::Albums && is_focused {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else if i == album_index {
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

    render_scrollable_list(frame, content_chunks[1], album_items, album_index, albums_block);
}

fn render_track_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    tracks: &[SearchTrack],
    selected_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
) {
    let border_style = if is_focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let content_width = area.width.saturating_sub(4) as usize;
    let track_items = render_detail_track_items(tracks, selected_index, is_focused, current_playing_uri, content_width, tracks.len());

    let list = List::new(track_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .padding(Padding::horizontal(1))
                .border_style(border_style),
        )
        .highlight_style(Style::default());

    let mut list_state = ListState::default();
    list_state.select(Some(selected_index + 1)); // +1 for header

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_album_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    albums: &[SearchAlbum],
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

fn render_artist_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    artists: &[SearchArtist],
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

fn render_queue(
    frame: &mut Frame,
    area: Rect,
    currently_playing: Option<&SearchTrack>,
    queue: &[SearchTrack],
    selected_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
) {
    let border_style = if is_focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Currently playing
            Constraint::Min(0),    // Queue
        ])
        .split(area);

    let cp_text = if let Some(track) = currently_playing {
        let liked = if track.liked { "💚 " } else { "" };
        let artists_display = if track.artists.len() > 1 {
            track.artists.join(", ")
        } else {
            track.artist.clone()
        };
        format!(
            "{}{}  -  {} ({})",
            liked,
            track.name,
            artists_display,
            track.album,
        )
    } else {
        "No track playing".to_string()
    };
    let cp_widget = Paragraph::new(cp_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL).title(" 🎵 Now Playing ").border_style(border_style));
    frame.render_widget(cp_widget, chunks[0]);

    let content_width = chunks[1].width.saturating_sub(4) as usize;
    let mut list_items = render_detail_track_items(queue, selected_index, is_focused, current_playing_uri, content_width, queue.len());

    if queue.is_empty() {
        list_items.push(
            ListItem::new("       Queue is empty")
                .style(Style::default().fg(Color::DarkGray))
        );
    }

    let queue_block = Block::default()
        .borders(Borders::ALL)
        .title(" Up Next ")
        .padding(Padding::horizontal(1))
        .border_style(border_style);

    render_scrollable_list(frame, chunks[1], list_items, selected_index + 1, queue_block);
}

fn render_detail_track_items(
    tracks: &[SearchTrack],
    selected_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
    content_width: usize,
    total_count: usize,
) -> Vec<ListItem<'static>> {
    let num_width = calculate_num_width(total_count);
    let liked_width = 2;
    let duration_width = 8;
    let fixed_width = 1 + num_width + 3 + liked_width + 3 + 3 + 3 + duration_width;
    let remaining_width = content_width.saturating_sub(fixed_width);
    let title_width = (remaining_width * 55) / 100;
    let artist_width = remaining_width.saturating_sub(title_width);

    // Create header as first item
    let mut items: Vec<ListItem<'static>> = vec![
        ListItem::new(format!(
            " {:<num_width$}   {}   {:<title_width$}   {:<artist_width$}   {}",
            "#", "  ", "Title", "Artist", "Duration",
            num_width = num_width,
            title_width = title_width,
            artist_width = artist_width
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let track_items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let duration = format_duration(track.duration_ms);
            let is_playing = current_playing_uri.map_or(false, |uri| uri == track.uri);
            let style = if i == selected_index && is_focused {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else if is_playing {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if i == selected_index {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let liked_indicator = if track.liked { "💚" } else { "  " };
            let playing_indicator = if is_playing { "▶" } else { " " };
            let track_num = format!("{}{:<num_width$}", playing_indicator, i + 1, num_width = num_width);

            let title_str = truncate_string(&track.name, title_width);
            let artists_display = if track.artists.len() > 1 {
                track.artists.join(", ")
            } else {
                track.artist.clone()
            };
            let artist_str = truncate_string(&artists_display, artist_width);

            ListItem::new(format!("{}   {}   {}   {}   {}", track_num, liked_indicator, title_str, artist_str, duration)).style(style)
        })
        .collect();

    items.extend(track_items);
    items
}

fn render_artist_track_items(
    tracks: &[SearchTrack],
    track_index: usize,
    is_focused: bool,
    current_playing_uri: Option<&str>,
    content_width: usize,
) -> Vec<ListItem<'static>> {
    let num_width = calculate_num_width(tracks.len());
    let liked_width = 2;
    let duration_width = 8;
    let fixed_track_width = 1 + num_width + 3 + liked_width + 3 + 3 + duration_width;
    let title_width_artist = content_width.saturating_sub(fixed_track_width);

    let mut items: Vec<ListItem<'static>> = vec![
        ListItem::new(format!(
            " {:<num_width$}   {}   {:<title_width_artist$}   {}",
            "#", "  ", "Title", "Duration",
            num_width = num_width,
            title_width_artist = title_width_artist
        ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    ];

    let track_items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let duration = format_duration(track.duration_ms);
            let is_playing = current_playing_uri.map_or(false, |uri| uri == track.uri);
            let style = if i == track_index && is_focused {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else if is_playing {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if i == track_index {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let liked_indicator = if track.liked { "💚" } else { "  " };
            let playing_indicator = if is_playing { "▶" } else { " " };
            let track_num = format!("{}{:<num_width$}", playing_indicator, i + 1, num_width = num_width);

            let title_str = truncate_string(&track.name, title_width_artist);

            ListItem::new(format!("{}   {}   {}   {}", track_num, liked_indicator, title_str, duration)).style(style)
        })
        .collect();

    items.extend(track_items);
    items
}
