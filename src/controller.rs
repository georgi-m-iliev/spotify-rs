use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use librespot::metadata::audio::UniqueFields;
use librespot::playback::player::{PlayerEvent, PlayerEventChannel};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::model::{ActiveSection, AppModel, SelectedItem, TrackMetadata};

pub struct AppController {
    model: Arc<Mutex<AppModel>>,
}

impl AppController {
    pub fn new(model: Arc<Mutex<AppModel>>) -> Self {
        Self { model }
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
            match spotify.search(query, 10).await {
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
                    if let Err(e) = spotify.play_track(&uri).await {
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

            let result = if is_playing {
                spotify.pause().await
            } else {
                spotify.play().await
            };

            if let Err(e) = result {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
            // Note: State will be updated via player events, no need to poll
        }
    }

    async fn next_track(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if let Err(e) = spotify.next_track().await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            }
        }
        // Note: State will be updated via player events
    }

    async fn previous_track(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if let Err(e) = spotify.previous_track().await {
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
