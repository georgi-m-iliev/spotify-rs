use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use librespot::metadata::audio::UniqueFields;
use librespot::playback::player::{PlayerEvent, PlayerEventChannel};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::audio::AudioBackend;
use crate::model::{ActiveSection, AppModel, SelectedItem, TrackMetadata};

pub const SEARCH_LIMIT: usize = 40;

pub struct AppController {
    model: Arc<Mutex<AppModel>>,
    audio_backend: Arc<AudioBackend>,
}

impl AppController {
    pub fn new(model: Arc<Mutex<AppModel>>, audio_backend: Arc<AudioBackend>) -> Self {
        Self { model, audio_backend }
    }

    pub async fn handle_key_event(&self, key: KeyEvent) -> Result<()> {
        // Only handle key press events, not release or repeat
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        let model = self.model.lock().await;
        let ui_state = model.get_ui_state().await;

        // Handle search input when in search section
        if ui_state.active_section == ActiveSection::Search {
            match key.code {
                KeyCode::Tab => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        model.cycle_section_backward().await;
                    } else {
                        model.cycle_section_forward().await;
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    // Trigger search
                    let query = ui_state.search_query.clone();
                    drop(model);
                    if !query.is_empty() {
                        self.perform_search(&query).await;
                    }
                    return Ok(());
                }
                KeyCode::Esc => {
                    model.update_search_query(String::new()).await;
                    return Ok(());
                }
                KeyCode::Backspace => {
                    model.backspace_search().await;
                    return Ok(());
                }
                KeyCode::Char(c) => {
                    // Q still quits even in search mode when Ctrl is pressed
                    if (c == 'q' || c == 'Q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        model.set_should_quit(true).await;
                        return Ok(());
                    }
                    model.append_to_search(c).await;
                    return Ok(());
                }
                _ => {}
            }
        }

        // Handle MainContent section navigation
        if ui_state.active_section == ActiveSection::MainContent {
            match key.code {
                KeyCode::Up => {
                    model.content_move_up().await;
                    return Ok(());
                }
                KeyCode::Down => {
                    model.content_move_down().await;
                    return Ok(());
                }
                KeyCode::Left => {
                    model.navigate_search_section(false).await;
                    return Ok(());
                }
                KeyCode::Right => {
                    model.navigate_search_section(true).await;
                    return Ok(());
                }
                KeyCode::Enter => {
                    // Open selected item or play track
                    let selected = model.get_selected_content_item().await;
                    drop(model);
                    if let Some(item) = selected {
                        self.handle_selected_item(item).await;
                    }
                    return Ok(());
                }
                KeyCode::Backspace | KeyCode::Esc => {
                    // Navigate back
                    model.navigate_back().await;
                    return Ok(());
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                model.set_should_quit(true).await;
            }
            KeyCode::Esc => {
                // Clear error message if one is displayed
                model.clear_error().await;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    model.cycle_section_backward().await;
                } else {
                    model.cycle_section_forward().await;
                }
            }
            KeyCode::BackTab => {
                model.cycle_section_backward().await;
            }
            KeyCode::Up => {
                model.move_selection_up().await;
            }
            KeyCode::Down => {
                model.move_selection_down().await;
            }
            KeyCode::Enter => {
                // Handle Enter based on active section
                let ui_state = model.get_ui_state().await;
                match ui_state.active_section {
                    ActiveSection::Library => {
                        // Open selected library item
                        let selected = ui_state.library_selected;
                        drop(model);
                        self.open_library_item(selected).await;
                        return Ok(());
                    }
                    ActiveSection::Playlists => {
                        // Open selected playlist
                        if let Some(playlist) = model.get_selected_playlist().await {
                            drop(model);
                            self.open_playlist(&playlist.id).await;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char(' ') => {
                drop(model); // Release lock before async operation
                self.toggle_playback().await;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                drop(model);
                self.next_track().await;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                drop(model);
                self.previous_track().await;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                drop(model);
                self.toggle_shuffle().await;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                drop(model);
                self.cycle_repeat().await;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                drop(model);
                self.volume_up().await;
            }
            KeyCode::Char('-') => {
                drop(model);
                self.volume_down().await;
            }
            _ => {}
        }
        Ok(())
    }

    async fn perform_search(&self, query: &str) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.search(query, SEARCH_LIMIT as u32).await {
                Ok(results) => {
                    model.set_search_results(results).await;
                    // Switch to MainContent section to show results
                    let mut ui_state = model.ui_state.lock().await;
                    ui_state.active_section = ActiveSection::MainContent;
                }
                Err(e) => {
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    async fn handle_selected_item(&self, item: SelectedItem) {
        let model = self.model.lock().await;

        match item {
            SelectedItem::Track { uri, .. } => {
                // Play the track
                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let uri_clone = uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let uri = uri_clone.clone();
                        async move { spotify.play_track(&uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::PlaylistTrack { playlist_uri, track_uri, .. } => {
                // Play playlist starting from selected track
                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let playlist_uri_clone = playlist_uri.clone();
                    let track_uri_clone = track_uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let playlist_uri = playlist_uri_clone.clone();
                        let track_uri = track_uri_clone.clone();
                        async move { spotify.play_context_from_track_uri(&playlist_uri, &track_uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::AlbumTrack { album_uri, track_uri, .. } => {
                // Play album starting from selected track
                if let Some(spotify) = &model.spotify {
                    let spotify_clone = spotify.clone();
                    let album_uri_clone = album_uri.clone();
                    let track_uri_clone = track_uri.clone();
                    drop(model);

                    let operation = move || {
                        let spotify = spotify_clone.clone();
                        let album_uri = album_uri_clone.clone();
                        let track_uri = track_uri_clone.clone();
                        async move { spotify.play_context_from_track_uri(&album_uri, &track_uri).await }
                    };

                    if let Err(e) = self.with_backend_recovery(operation).await {
                        let model = self.model.lock().await;
                        let error_msg = Self::format_error(&e);
                        model.set_error(error_msg).await;
                    }
                }
            }
            SelectedItem::Album { id, .. } => {
                // Open album detail
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_album(&id).await {
                        Ok(detail) => {
                            model.set_album_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
            SelectedItem::Artist { id, .. } => {
                // Open artist detail
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_artist(&id).await {
                        Ok(detail) => {
                            model.set_artist_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
            SelectedItem::Playlist { id, .. } => {
                // Open playlist detail
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_playlist(&id).await {
                        Ok(detail) => {
                            model.set_playlist_detail(detail).await;
                        }
                        Err(e) => {
                            model.set_content_loading(false).await;
                            let error_msg = Self::format_error(&e);
                            model.set_error(error_msg).await;
                        }
                    }
                }
            }
        }
    }

    async fn toggle_playback(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let is_playing = model.is_playing().await;
            let spotify_clone = spotify.clone();

            drop(model); // Release lock before potentially slow operations

            let operation = move || {
                let spotify = spotify_clone.clone();
                let playing = is_playing;
                async move {
                    if playing {
                        spotify.pause().await
                    } else {
                        spotify.play().await
                    }
                }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
            // Note: State will be updated via player events, no need to poll
        }
    }

    async fn next_track(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                async move { spotify.next_track().await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
        }
        // Note: State will be updated via player events
    }

    async fn previous_track(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                async move { spotify.previous_track().await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
        }
        // Note: State will be updated via player events
    }

    async fn toggle_shuffle(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_shuffle = model.get_shuffle_state().await;
            let new_shuffle = !current_shuffle;

            if let Err(e) = spotify.set_shuffle(new_shuffle).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Update local state
                model.set_shuffle(new_shuffle).await;
            }
        }
    }

    async fn cycle_repeat(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_repeat = model.get_repeat_state().await;
            let new_repeat = match current_repeat {
                crate::model::RepeatState::Off => crate::model::RepeatState::All,
                crate::model::RepeatState::All => crate::model::RepeatState::One,
                crate::model::RepeatState::One => crate::model::RepeatState::Off,
            };

            if let Err(e) = spotify.set_repeat(new_repeat).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Update local state
                model.set_repeat(new_repeat).await;
            }
        }
    }

    async fn volume_up(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_volume = model.get_volume().await;
            let new_volume = (current_volume + 5).min(100);

            if let Err(e) = spotify.set_volume(new_volume).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Update local state
                model.set_volume(new_volume).await;
            }
        }
    }

    async fn volume_down(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_volume = model.get_volume().await;
            let new_volume = current_volume.saturating_sub(5);

            if let Err(e) = spotify.set_volume(new_volume).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Update local state
                model.set_volume(new_volume).await;
            }
        }
    }

    /// Refresh playback state from Spotify API (fallback/initial sync)
    pub async fn refresh_playback(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_current_playback().await {
                Ok(Some(playback)) => {
                    model.update_from_playback_context(&playback).await;
                }
                Ok(None) => {
                    // No active playback, this is fine
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    /// Load user's playlists from Spotify API
    pub async fn load_user_playlists(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_user_playlists(50).await {
                Ok(playlists) => {
                    model.set_playlists(playlists).await;
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    /// Open a playlist by ID to show its details
    async fn open_playlist(&self, playlist_id: &str) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_playlist(playlist_id).await {
                Ok(detail) => {
                    model.set_playlist_detail(detail).await;
                    // Switch to MainContent section to show playlist details
                    let mut ui_state = model.ui_state.lock().await;
                    ui_state.active_section = ActiveSection::MainContent;
                }
                Err(e) => {
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    /// Open a library item by its index
    /// 0 = Recently played, 1 = Liked songs, 2 = Albums, 3 = Artists
    async fn open_library_item(&self, index: usize) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            let result = match index {
                0 => {
                    // Recently played
                    match spotify.get_recently_played(50).await {
                        Ok(tracks) => {
                            model.set_recently_played(tracks).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                1 => {
                    // Liked songs
                    match spotify.get_liked_songs(100).await {
                        Ok(tracks) => {
                            model.set_liked_songs(tracks).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                2 => {
                    // Albums
                    match spotify.get_saved_albums(50).await {
                        Ok(albums) => {
                            model.set_saved_albums(albums).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                3 => {
                    // Artists
                    match spotify.get_followed_artists(50).await {
                        Ok(artists) => {
                            model.set_followed_artists(artists).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => {
                    model.set_content_loading(false).await;
                    return;
                }
            };

            if let Err(e) = result {
                model.set_content_loading(false).await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                // Switch to MainContent section to show results
                let mut ui_state = model.ui_state.lock().await;
                ui_state.active_section = ActiveSection::MainContent;
            }
        }
    }

    /// Play a playlist from the beginning
    pub async fn play_playlist_from_start(&self, uri: &str) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            let uri_clone = uri.to_string();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                let uri = uri_clone.clone();
                async move { spotify.play_context(&uri).await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
        }
    }

    /// Play a playlist starting from a specific track
    pub async fn play_playlist_from_track(&self, playlist_uri: &str, track_uri: &str) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            let playlist_uri_clone = playlist_uri.to_string();
            let track_uri_clone = track_uri.to_string();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                let playlist_uri = playlist_uri_clone.clone();
                let track_uri = track_uri_clone.clone();
                async move { spotify.play_context_from_track_uri(&playlist_uri, &track_uri).await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
        }
    }

    /// Format error messages to be user-friendly
    fn format_error(error: &anyhow::Error) -> String {
        let error_str = error.to_string();

        // Handle common Spotify API errors
        if error_str.contains("404") {
            "No active device found. Start playing on Spotify and try again.".to_string()
        } else if error_str.contains("403") {
            "Action forbidden. Check your Spotify Premium status.".to_string()
        } else if error_str.contains("401") {
            "Authentication expired. Please restart the app.".to_string()
        } else if error_str.contains("429") {
            "Rate limited. Please wait a moment.".to_string()
        } else if error_str.contains("Player command failed") {
            "No active playback. Start playing a song first.".to_string()
        } else {
            // Generic error message
            format!("Error: {}", error_str)
        }
    }

    /// Check if the error indicates the audio backend needs to be restarted
    fn is_device_unavailable_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("404")
            || error_str.contains("no active device")
            || error_str.contains("device not found")
            || error_str.contains("player command failed")
    }

    /// Try to restart the audio backend and return a new event channel if successful
    pub async fn try_restart_audio_backend(&self) -> Option<PlayerEventChannel> {
        {
            let model = self.model.lock().await;
            model.set_error("Reconnecting audio...".to_string()).await;
        }

        match self.audio_backend.restart().await {
            Ok(event_channel) => {
                // Wait a bit for device registration with Spotify
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                {
                    let model = self.model.lock().await;
                    model.clear_error().await;
                }
                Some(event_channel)
            }
            Err(e) => {
                let model = self.model.lock().await;
                model.set_error(format!("Audio reconnect failed: {}", e)).await;
                None
            }
        }
    }

    /// Execute a playback operation with automatic backend recovery on failure
    async fn with_backend_recovery<F, Fut>(&self, operation: F) -> Result<()>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<()>>,
    {
        // First attempt
        match operation().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Check if this is a device unavailable error
                if Self::is_device_unavailable_error(&e) {
                    // Try to restart the backend
                    if let Some(event_channel) = self.try_restart_audio_backend().await {
                        // Start listening to events from the new backend
                        self.start_player_event_listener(event_channel);

                        // Wait a bit more for stability
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Retry the operation
                        return operation().await;
                    }
                }
                Err(e)
            }
        }
    }

    /// Start listening to librespot player events for real-time playback updates
    pub fn start_player_event_listener(&self, mut event_channel: PlayerEventChannel) {
        let model = self.model.clone();

        tokio::spawn(async move {
            while let Some(event) = event_channel.recv().await {
                let model_guard = model.lock().await;

                if model_guard.should_quit().await {
                    break;
                }

                match event {
                    PlayerEvent::Playing { position_ms, .. } => {
                        model_guard.update_playback_position(position_ms, true).await;
                    }
                    PlayerEvent::Paused { position_ms, .. } => {
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::PositionChanged { position_ms, .. } => {
                        // Periodic position update - keep current playing state
                        let is_playing = model_guard.is_playing().await;
                        model_guard.update_playback_position(position_ms, is_playing).await;
                    }
                    PlayerEvent::Seeked { position_ms, .. } => {
                        let is_playing = model_guard.is_playing().await;
                        model_guard.update_playback_position(position_ms, is_playing).await;
                    }
                    PlayerEvent::TrackChanged { audio_item } => {
                        // Extract artist and album from unique_fields based on content type
                        let (artist, album) = match &audio_item.unique_fields {
                            UniqueFields::Track { artists, album, .. } => {
                                let artist_name = artists
                                    .0
                                    .first()
                                    .map(|a| a.name.clone())
                                    .unwrap_or_default();
                                (artist_name, album.clone())
                            }
                            UniqueFields::Episode { show_name, .. } => {
                                (show_name.clone(), "Podcast".to_string())
                            }
                            UniqueFields::Local { artists, album, .. } => {
                                let artist_name = artists.clone().unwrap_or_default();
                                let album_name = album.clone().unwrap_or_default();
                                (artist_name, album_name)
                            }
                        };

                        let track = TrackMetadata {
                            name: audio_item.name.clone(),
                            artist,
                            album,
                            duration_ms: audio_item.duration_ms,
                        };
                        model_guard.update_track_info(track).await;
                    }
                    PlayerEvent::Stopped { .. } => {
                        model_guard.update_playback_position(0, false).await;
                    }
                    PlayerEvent::Loading { position_ms, .. } => {
                        // Track is loading, update position
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::EndOfTrack { .. } => {
                        // Track ended, will transition to next track
                        model_guard.set_playing(false).await;
                    }
                    _ => {
                        // Ignore other events (volume, session, shuffle, repeat, etc.)
                    }
                }
            }
        });
    }
}
