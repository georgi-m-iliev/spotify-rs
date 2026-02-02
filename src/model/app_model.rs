//! Main application model with state management

use std::sync::Arc;
use std::collections::HashSet;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use rspotify::model::CurrentPlaybackContext;

use super::types::{
    ActiveSection, ArtistDetailSection, DeviceInfo, PlaylistItem, 
    RepeatState, SearchResultSection, SelectedItem, UiState,
};
use super::playback::{PlaybackInfo, PlaybackSettings, PlaybackTiming, TrackMetadata};
use super::content::{
    AlbumDetail, ArtistDetail, ContentState, ContentView, PlaylistDetail,
    SearchAlbum, SearchArtist, SearchResults, SearchTrack,
};
use super::spotify_client::SpotifyClient;

/// Main application model containing all state
pub struct AppModel {
    pub spotify: Option<SpotifyClient>,
    track_metadata: Arc<Mutex<TrackMetadata>>,
    playback_timing: Arc<Mutex<PlaybackTiming>>,
    playback_settings: Arc<Mutex<PlaybackSettings>>,
    pub ui_state: Arc<Mutex<UiState>>,
    pub content_state: Arc<Mutex<ContentState>>,
    pub should_quit: Arc<Mutex<bool>>,
    queue_skip_list: Arc<RwLock<HashSet<String>>>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            spotify: None,
            track_metadata: Arc::new(Mutex::new(TrackMetadata::default())),
            playback_timing: Arc::new(Mutex::new(PlaybackTiming::default())),
            playback_settings: Arc::new(Mutex::new(PlaybackSettings::default())),
            ui_state: Arc::new(Mutex::new(UiState::default())),
            content_state: Arc::new(Mutex::new(ContentState::default())),
            should_quit: Arc::new(Mutex::new(false)),
            queue_skip_list: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn set_spotify_client(&mut self, client: SpotifyClient) {
        self.spotify = Some(client);
    }

    pub async fn get_spotify_client(&self) -> Option<SpotifyClient> {
        self.spotify.clone()
    }

    // ========================================================================
    // Device & Playback State
    // ========================================================================

    pub async fn update_device_name(&self, name: String) {
        let mut settings = self.playback_settings.lock().await;
        settings.device_name = name;
    }

    pub async fn update_track_info(&self, track: TrackMetadata) {
        let duration_ms = track.duration_ms;
        *self.track_metadata.lock().await = track;

        let mut timing = self.playback_timing.lock().await;
        timing.duration_ms = duration_ms;
    }

    pub async fn update_playback_position(&self, position_ms: u32, is_playing: bool) {
        let mut timing = self.playback_timing.lock().await;
        timing.update_position(position_ms, is_playing);
    }

    pub async fn set_playing(&self, is_playing: bool) {
        let mut timing = self.playback_timing.lock().await;
        timing.position_ms = timing.current_position_ms();
        timing.is_playing = is_playing;
        timing.last_update = Instant::now();
    }

    pub async fn update_from_playback_context(&self, playback: &CurrentPlaybackContext) {
        let track = TrackMetadata::from_playback(playback);
        let progress_ms = playback
            .progress
            .map(|d| d.num_milliseconds() as u32)
            .unwrap_or(0);
        let is_playing = playback.is_playing;

        *self.track_metadata.lock().await = track.clone();

        let mut timing = self.playback_timing.lock().await;
        timing.position_ms = progress_ms;
        timing.duration_ms = track.duration_ms;
        timing.is_playing = is_playing;
        timing.last_update = Instant::now();
        drop(timing);

        let mut settings = self.playback_settings.lock().await;
        settings.shuffle = playback.shuffle_state;
        settings.repeat = match playback.repeat_state {
            rspotify::model::RepeatState::Off => RepeatState::Off,
            rspotify::model::RepeatState::Track => RepeatState::One,
            rspotify::model::RepeatState::Context => RepeatState::All,
        };

        if let Some(volume) = playback.device.volume_percent {
            settings.volume = volume as u8;
        }
    }

    pub async fn get_playback_info(&self) -> PlaybackInfo {
        let track = self.track_metadata.lock().await.clone();
        let timing = self.playback_timing.lock().await;
        let settings = self.playback_settings.lock().await.clone();

        PlaybackInfo {
            track,
            progress_ms: timing.current_position_ms(),
            duration_ms: timing.duration_ms,
            is_playing: timing.is_playing,
            settings,
        }
    }

    pub async fn is_playing(&self) -> bool {
        self.playback_timing.lock().await.is_playing
    }

    pub async fn get_shuffle_state(&self) -> bool {
        self.playback_settings.lock().await.shuffle
    }

    pub async fn set_shuffle(&self, shuffle: bool) {
        let mut settings = self.playback_settings.lock().await;
        settings.shuffle = shuffle;
    }

    pub async fn get_repeat_state(&self) -> RepeatState {
        self.playback_settings.lock().await.repeat
    }

    pub async fn set_repeat(&self, repeat: RepeatState) {
        let mut settings = self.playback_settings.lock().await;
        settings.repeat = repeat;
    }

    pub async fn get_volume(&self) -> u8 {
        self.playback_settings.lock().await.volume
    }

    pub async fn set_volume(&self, volume: u8) {
        let mut settings = self.playback_settings.lock().await;
        settings.volume = volume;
    }

    pub async fn should_quit(&self) -> bool {
        *self.should_quit.lock().await
    }

    pub async fn set_should_quit(&self, quit: bool) {
        *self.should_quit.lock().await = quit;
    }

    pub async fn get_ui_state(&self) -> UiState {
        self.ui_state.lock().await.clone()
    }

    pub async fn cycle_section_forward(&self) {
        let mut state = self.ui_state.lock().await;
        state.active_section = state.active_section.next();
    }

    pub async fn cycle_section_backward(&self) {
        let mut state = self.ui_state.lock().await;
        state.active_section = state.active_section.prev();
    }

    pub async fn set_active_section(&self, section: ActiveSection) {
        let mut state = self.ui_state.lock().await;
        state.active_section = section;
    }

    pub async fn move_selection_up(&self) {
        let mut state = self.ui_state.lock().await;
        match state.active_section {
            ActiveSection::Library => {
                if state.library_selected > 0 {
                    state.library_selected -= 1;
                }
            }
            ActiveSection::Playlists => {
                if state.playlist_selected > 0 {
                    state.playlist_selected -= 1;
                }
            }
            _ => {}
        }
    }

    pub async fn move_selection_down(&self) {
        let mut state = self.ui_state.lock().await;
        match state.active_section {
            ActiveSection::Library => {
                if state.library_selected < state.library_items.len().saturating_sub(1) {
                    state.library_selected += 1;
                }
            }
            ActiveSection::Playlists => {
                if state.playlist_selected < state.playlists.len().saturating_sub(1) {
                    state.playlist_selected += 1;
                }
            }
            _ => {}
        }
    }

    pub async fn update_search_query(&self, query: String) {
        let mut state = self.ui_state.lock().await;
        state.search_query = query;
    }

    pub async fn append_to_search(&self, c: char) {
        let mut state = self.ui_state.lock().await;
        state.search_query.push(c);
    }

    pub async fn backspace_search(&self) {
        let mut state = self.ui_state.lock().await;
        state.search_query.pop();
    }

    pub async fn set_playlists(&self, playlists: Vec<PlaylistItem>) {
        let mut state = self.ui_state.lock().await;
        state.playlists = playlists;
        state.playlist_selected = 0;
    }

    pub async fn get_selected_playlist(&self) -> Option<PlaylistItem> {
        let state = self.ui_state.lock().await;
        state.playlists.get(state.playlist_selected).cloned()
    }

    pub async fn set_error(&self, message: String) {
        let mut state = self.ui_state.lock().await;
        state.error_message = Some(message);
        state.error_timestamp = Some(Instant::now());
    }

    pub async fn clear_error(&self) {
        let mut state = self.ui_state.lock().await;
        state.error_message = None;
        state.error_timestamp = None;
    }

    pub async fn has_error(&self) -> bool {
        self.ui_state.lock().await.error_message.is_some()
    }

    pub async fn auto_clear_old_errors(&self) {
        let mut state = self.ui_state.lock().await;
        if let Some(timestamp) = state.error_timestamp {
            if timestamp.elapsed().as_secs() > 5 {
                state.error_message = None;
                state.error_timestamp = None;
            }
        }
    }

    pub async fn show_device_picker(&self, devices: Vec<DeviceInfo>) {
        let mut state = self.ui_state.lock().await;
        let active_index = devices.iter().position(|d| d.is_active).unwrap_or(0);
        state.available_devices = devices;
        state.device_selected = active_index;
        state.show_device_picker = true;
    }

    pub async fn hide_device_picker(&self) {
        let mut state = self.ui_state.lock().await;
        state.show_device_picker = false;
    }

    pub async fn is_device_picker_open(&self) -> bool {
        self.ui_state.lock().await.show_device_picker
    }

    pub async fn device_picker_move_up(&self) {
        let mut state = self.ui_state.lock().await;
        if state.device_selected > 0 {
            state.device_selected -= 1;
        }
    }

    pub async fn device_picker_move_down(&self) {
        let mut state = self.ui_state.lock().await;
        if state.device_selected < state.available_devices.len().saturating_sub(1) {
            state.device_selected += 1;
        }
    }

    pub async fn get_selected_device(&self) -> Option<DeviceInfo> {
        let state = self.ui_state.lock().await;
        state.available_devices.get(state.device_selected).cloned()
    }

    pub async fn get_local_device_name(&self) -> String {
        self.playback_settings.lock().await.device_name.clone()
    }

    pub async fn show_help_popup(&self) {
        let mut state = self.ui_state.lock().await;
        state.show_help_popup = true;
    }

    pub async fn hide_help_popup(&self) {
        let mut state = self.ui_state.lock().await;
        state.show_help_popup = false;
    }

    pub async fn is_help_popup_open(&self) -> bool {
        self.ui_state.lock().await.show_help_popup
    }

    pub async fn get_content_state(&self) -> ContentState {
        self.content_state.lock().await.clone()
    }

    pub async fn set_search_results(&self, results: SearchResults) {
        let mut state = self.content_state.lock().await;

        if !matches!(state.view, ContentView::Empty) {
            state.navigation_stack.clear(); // Clear stack on new search
        }
        // Use the best matching category as the initial section
        let initial_section = results.best_match;
        state.view = ContentView::SearchResults {
            results,
            section: initial_section,
            track_index: 0,
            album_index: 0,
            artist_index: 0,
            playlist_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_album_detail(&self, detail: AlbumDetail) {
        let mut state = self.content_state.lock().await;

        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::AlbumDetail {
            detail,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_playlist_detail(&self, detail: PlaylistDetail) {
        let mut state = self.content_state.lock().await;

        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::PlaylistDetail {
            detail,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_artist_detail(&self, detail: ArtistDetail) {
        let mut state = self.content_state.lock().await;

        if !matches!(state.view, ContentView::Empty) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::ArtistDetail {
            detail,
            section: ArtistDetailSection::TopTracks,
            track_index: 0,
            album_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_liked_songs(&self, tracks: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;

        state.navigation_stack.clear();
        state.view = ContentView::LikedSongs {
            tracks,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_saved_albums(&self, albums: Vec<SearchAlbum>) {
        let mut state = self.content_state.lock().await;

        state.navigation_stack.clear();
        state.view = ContentView::SavedAlbums {
            albums,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_followed_artists(&self, artists: Vec<SearchArtist>) {
        let mut state = self.content_state.lock().await;

        state.navigation_stack.clear();
        state.view = ContentView::FollowedArtists {
            artists,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_recently_played(&self, tracks: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;

        state.navigation_stack.clear();
        state.view = ContentView::RecentlyPlayed {
            tracks,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn set_queue(&self, currently_playing: Option<SearchTrack>, queue: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;

        if !matches!(state.view, ContentView::Empty | ContentView::Queue { .. }) {
            let previous_view = state.view.clone();
            state.navigation_stack.push(previous_view);
        }
        state.view = ContentView::Queue {
            currently_playing,
            queue,
            selected_index: 0,
        };
        state.is_loading = false;
    }

    pub async fn is_queue_view_visible(&self) -> bool {
        let state = self.content_state.lock().await;
        matches!(state.view, ContentView::Queue { .. })
    }

    pub async fn update_queue_if_visible(&self, currently_playing: Option<SearchTrack>, queue: Vec<SearchTrack>) {
        let mut state = self.content_state.lock().await;
        if let ContentView::Queue { selected_index, .. } = &state.view {
            let current_index = *selected_index;
            // Preserve selection but clamp to new queue size
            let new_index = current_index.min(queue.len().saturating_sub(1));
            state.view = ContentView::Queue {
                currently_playing,
                queue,
                selected_index: new_index,
            };
        }
    }

    pub async fn remove_from_queue_view(&self, index: usize) -> Option<String> {
        let mut state = self.content_state.lock().await;
        if let ContentView::Queue { queue, selected_index, .. } = &mut state.view {
            if index < queue.len() {
                let removed = queue.remove(index);
                // Adjust selected index if needed
                if *selected_index >= queue.len() && !queue.is_empty() {
                    *selected_index = queue.len() - 1;
                }
                return Some(removed.uri);
            }
        }
        None
    }

    pub async fn add_to_queue_skip_list(&self, uri: String) {
        let mut skip_list = self.queue_skip_list.write().await;
        tracing::debug!(uri = %uri, "Adding track to queue skip list");
        skip_list.insert(uri);
    }

    pub async fn is_in_queue_skip_list(&self, uri: &str) -> bool {
        let skip_list = self.queue_skip_list.read().await;
        skip_list.contains(uri)
    }

    pub async fn remove_from_queue_skip_list(&self, uri: &str) {
        let mut skip_list = self.queue_skip_list.write().await;
        skip_list.remove(uri);
    }

    pub async fn clear_queue_skip_list(&self) {
        let mut skip_list = self.queue_skip_list.write().await;
        if !skip_list.is_empty() {
            tracing::debug!(count = skip_list.len(), "Clearing queue skip list");
            skip_list.clear();
        }
    }

    pub async fn set_content_loading(&self, loading: bool) {
        let mut state = self.content_state.lock().await;
        state.is_loading = loading;
    }

    pub async fn navigate_back(&self) -> bool {
        let mut state = self.content_state.lock().await;
        if let Some(previous_view) = state.navigation_stack.pop() {
            // Restore the previous view
            state.view = previous_view;
            true
        } else {
            // No navigation history - go back to empty view
            state.view = ContentView::Empty;
            false
        }
    }

    pub async fn navigate_search_section(&self, forward: bool) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults { section, .. } => {
                *section = if forward { section.next() } else { section.prev() };
            }
            ContentView::ArtistDetail { section, .. } => {
                // Artist detail only has 2 sections, so forward/backward both toggle
                *section = section.next();
            }
            _ => {}
        }
    }

    pub async fn content_move_up(&self) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults {
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
                ..
            } => {
                let idx = match section {
                    SearchResultSection::Tracks => track_index,
                    SearchResultSection::Albums => album_index,
                    SearchResultSection::Artists => artist_index,
                    SearchResultSection::Playlists => playlist_index,
                };
                if *idx > 0 {
                    *idx -= 1;
                }
            }
            ContentView::AlbumDetail { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::PlaylistDetail { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::ArtistDetail {
                section,
                track_index,
                album_index,
                ..
            } => {
                let idx = match section {
                    ArtistDetailSection::TopTracks => track_index,
                    ArtistDetailSection::Albums => album_index,
                };
                if *idx > 0 {
                    *idx -= 1;
                }
            }
            ContentView::LikedSongs { selected_index, .. }
            | ContentView::RecentlyPlayed { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::SavedAlbums { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::FollowedArtists { selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
            }
            ContentView::Queue { queue, selected_index, .. } => {
                if *selected_index > 0 {
                    *selected_index -= 1;
                }
                let _ = queue; // silence unused warning
            }
            ContentView::Empty => {}
        }
    }

    pub async fn content_move_down(&self) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults {
                results,
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
            } => {
                let (idx, max) = match section {
                    SearchResultSection::Tracks => (track_index, results.tracks.len()),
                    SearchResultSection::Albums => (album_index, results.albums.len()),
                    SearchResultSection::Artists => (artist_index, results.artists.len()),
                    SearchResultSection::Playlists => (playlist_index, results.playlists.len()),
                };
                if *idx < max.saturating_sub(1) {
                    *idx += 1;
                }
            }
            ContentView::AlbumDetail { detail, selected_index } => {
                if *selected_index < detail.tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                if *selected_index < detail.tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::ArtistDetail {
                detail,
                section,
                track_index,
                album_index,
            } => {
                let (idx, max) = match section {
                    ArtistDetailSection::TopTracks => (track_index, detail.top_tracks.len()),
                    ArtistDetailSection::Albums => (album_index, detail.albums.len()),
                };
                if *idx < max.saturating_sub(1) {
                    *idx += 1;
                }
            }
            ContentView::LikedSongs { tracks, selected_index } => {
                if *selected_index < tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                if *selected_index < tracks.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::SavedAlbums { albums, selected_index } => {
                if *selected_index < albums.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::FollowedArtists { artists, selected_index } => {
                if *selected_index < artists.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::Queue { queue, selected_index, .. } => {
                if *selected_index < queue.len().saturating_sub(1) {
                    *selected_index += 1;
                }
            }
            ContentView::Empty => {}
        }
    }

    pub async fn get_selected_content_item(&self) -> Option<SelectedItem> {
        let state = self.content_state.lock().await;
        match &state.view {
            ContentView::SearchResults {
                results,
                section,
                track_index,
                album_index,
                artist_index,
                playlist_index,
            } => match section {
                SearchResultSection::Tracks => results.tracks.get(*track_index).map(|t| {
                    SelectedItem::Track { uri: t.uri.clone() }
                }),
                SearchResultSection::Albums => results.albums.get(*album_index).map(|a| {
                    SelectedItem::Album { id: a.id.clone() }
                }),
                SearchResultSection::Artists => results.artists.get(*artist_index).map(|a| {
                    SelectedItem::Artist { id: a.id.clone() }
                }),
                SearchResultSection::Playlists => results.playlists.get(*playlist_index).map(|p| {
                    SelectedItem::Playlist { id: p.id.clone() }
                }),
            },
            ContentView::AlbumDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| SelectedItem::AlbumTrack {
                    album_uri: format!("spotify:album:{}", detail.id),
                    track_uri: t.uri.clone(),
                })
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| SelectedItem::PlaylistTrack {
                    playlist_uri: detail.uri.clone(),
                    track_uri: t.uri.clone(),
                })
            }
            ContentView::ArtistDetail {
                detail,
                section,
                track_index,
                album_index,
            } => match section {
                ArtistDetailSection::TopTracks => detail.top_tracks.get(*track_index).map(|t| {
                    SelectedItem::Track { uri: t.uri.clone() }
                }),
                ArtistDetailSection::Albums => detail.albums.get(*album_index).map(|a| {
                    SelectedItem::Album { id: a.id.clone() }
                }),
            },
            ContentView::LikedSongs { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| SelectedItem::Track { uri: t.uri.clone(), })
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| SelectedItem::Track { uri: t.uri.clone() })
            }
            ContentView::SavedAlbums { albums, selected_index } => {
                albums.get(*selected_index).map(|a| SelectedItem::Album { id: a.id.clone() })
            }
            ContentView::FollowedArtists { artists, selected_index } => {
                artists.get(*selected_index).map(|a| SelectedItem::Artist { id: a.id.clone() })
            }
            ContentView::Queue { queue, selected_index, .. } => {
                queue.get(*selected_index).map(|t| SelectedItem::Track { uri: t.uri.clone() })
            }
            ContentView::Empty => None,
        }
    }

    const PAGINATION_THRESHOLD: usize = 10;

    pub async fn should_load_more_playlist_tracks(&self) -> Option<(String, usize)> {
        let state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, selected_index } = &state.view {
            // Don't load if already loading or no more tracks
            if detail.loading_more || !detail.has_more {
                return None;
            }

            // Check if we're within threshold of the end
            let loaded_count = detail.tracks.len();
            if *selected_index + Self::PAGINATION_THRESHOLD >= loaded_count {
                return Some((detail.id.clone(), loaded_count));
            }
        }
        None
    }

    pub async fn set_playlist_loading_more(&self, loading: bool) {
        let mut state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, .. } = &mut state.view {
            detail.loading_more = loading;
        }
    }

    pub async fn append_playlist_tracks(&self, mut new_tracks: Vec<SearchTrack>, has_more: bool) {
        let mut state = self.content_state.lock().await;
        if let ContentView::PlaylistDetail { detail, .. } = &mut state.view {
            detail.tracks.append(&mut new_tracks);
            detail.has_more = has_more;
            detail.loading_more = false;
        }
    }

    pub async fn get_selected_track_for_like(&self) -> Option<(String, bool)> {
        let state = self.content_state.lock().await;
        let result = match &state.view {
            ContentView::SearchResults { results, section, track_index, .. } => {
                if *section == SearchResultSection::Tracks {
                    results.tracks.get(*track_index).map(|t| (t.id.clone(), t.liked))
                } else {
                    None
                }
            }
            ContentView::AlbumDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| (t.id.clone(), t.liked))
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| (t.id.clone(), t.liked))
            }
            ContentView::ArtistDetail { detail, section, track_index, .. } => {
                if *section == ArtistDetailSection::TopTracks {
                    detail.top_tracks.get(*track_index).map(|t| (t.id.clone(), t.liked))
                } else {
                    None
                }
            }
            ContentView::LikedSongs { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| (t.id.clone(), t.liked))
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| (t.id.clone(), t.liked))
            }
            ContentView::Queue { queue, selected_index, .. } => {
                queue.get(*selected_index).map(|t| (t.id.clone(), t.liked))
            }
            _ => None,
        };

        if let Some((ref id, liked)) = result {
            tracing::debug!(track_id = %id, liked, "Selected track for like toggle");
        }

        result
    }

    pub async fn update_track_liked_status(&self, track_id: &str, liked: bool) {
        let mut state = self.content_state.lock().await;
        match &mut state.view {
            ContentView::SearchResults { results, .. } => {
                if let Some(track) = results.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::AlbumDetail { detail, .. } => {
                if let Some(track) = detail.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::PlaylistDetail { detail, .. } => {
                if let Some(track) = detail.tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::ArtistDetail { detail, .. } => {
                if let Some(track) = detail.top_tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::LikedSongs { tracks, .. } => {
                if let Some(track) = tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::RecentlyPlayed { tracks, .. } => {
                if let Some(track) = tracks.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
            }
            ContentView::Queue { queue, currently_playing, .. } => {
                if let Some(track) = queue.iter_mut().find(|t| t.id == track_id) {
                    track.liked = liked;
                }
                if let Some(cp) = currently_playing {
                    if cp.id == track_id {
                        cp.liked = liked;
                    }
                }
            }
            _ => {}
        }
    }

    pub async fn get_selected_queue_index(&self) -> Option<usize> {
        let state = self.content_state.lock().await;
        if let ContentView::Queue { selected_index, .. } = &state.view {
            Some(*selected_index)
        } else {
            None
        }
    }

    pub async fn get_selected_track_uri(&self) -> Option<String> {
        let state = self.content_state.lock().await;
        match &state.view {
            ContentView::SearchResults { results, section, track_index, .. } => {
                if *section == SearchResultSection::Tracks {
                    results.tracks.get(*track_index).map(|t| t.uri.clone())
                } else {
                    None
                }
            }
            ContentView::AlbumDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| t.uri.clone())
            }
            ContentView::PlaylistDetail { detail, selected_index } => {
                detail.tracks.get(*selected_index).map(|t| t.uri.clone())
            }
            ContentView::ArtistDetail { detail, section, track_index, .. } => {
                if *section == ArtistDetailSection::TopTracks {
                    detail.top_tracks.get(*track_index).map(|t| t.uri.clone())
                } else {
                    None
                }
            }
            ContentView::LikedSongs { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| t.uri.clone())
            }
            ContentView::RecentlyPlayed { tracks, selected_index } => {
                tracks.get(*selected_index).map(|t| t.uri.clone())
            }
            ContentView::Queue { queue, selected_index, .. } => {
                queue.get(*selected_index).map(|t| t.uri.clone())
            }
            _ => None,
        }
    }
}

impl Default for AppModel {
    fn default() -> Self {
        Self::new()
    }
}
