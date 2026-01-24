use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use librespot::metadata::audio::UniqueFields;
use librespot::playback::player::{PlayerEvent, PlayerEventChannel};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

use crate::audio::AudioBackend;
use crate::model::{ActiveSection, AppModel, SelectedItem, TrackMetadata};

pub const SEARCH_LIMIT: usize = 40;

#[derive(Clone)]
pub struct AppController {
    model: Arc<Mutex<AppModel>>,
    audio_backend: Arc<Mutex<Option<AudioBackend>>>,
    event_listener_started: Arc<Mutex<bool>>,
}

impl AppController {
    pub fn new(model: Arc<Mutex<AppModel>>, audio_backend: Arc<Mutex<Option<AudioBackend>>>) -> Self {
        Self {
            model,
            audio_backend,
            event_listener_started: Arc::new(Mutex::new(false)),
        }
    }

    /// Try to start the player event listener if backend is ready and not already started
    async fn try_start_event_listener(&self) {
        let mut started = self.event_listener_started.lock().await;
        if *started {
            return;
        }

        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            if let Some(event_channel) = backend.get_player_event_channel().await {
                *started = true;
                drop(backend_guard);
                drop(started);
                self.start_player_event_listener(event_channel);
            }
        }
    }

    pub async fn handle_key_event(&self, key: KeyEvent) -> Result<()> {
        // Only handle key press events, not release or repeat
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        let model = self.model.lock().await;

        // Handle device picker modal first (highest priority)
        if model.is_device_picker_open().await {
            return match key.code {
                KeyCode::Up => {
                    model.device_picker_move_up().await;
                    Ok(())
                }
                KeyCode::Down => {
                    model.device_picker_move_down().await;
                    Ok(())
                }
                KeyCode::Enter => {
                    // Select the device
                    if let Some(device) = model.get_selected_device().await {
                        let local_device_name = model.get_local_device_name().await;
                        model.hide_device_picker().await;
                        drop(model);
                        self.select_device(&device, &local_device_name).await;
                    }
                    Ok(())
                }
                KeyCode::Esc | KeyCode::Char('d') | KeyCode::Char('D') => {
                    model.hide_device_picker().await;
                    Ok(())
                }
                _ => Ok(()),
            }
        }

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
            KeyCode::Char('d') | KeyCode::Char('D') => {
                drop(model);
                self.open_device_picker().await;
            }
            _ => {}
        }
        Ok(())
    }

    async fn perform_search(&self, query: &str) {
        debug!(query, "Performing search");
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.search(query, SEARCH_LIMIT as u32).await {
                Ok(mut results) => {
                    info!(
                        query,
                        tracks = results.tracks.len(),
                        albums = results.albums.len(),
                        artists = results.artists.len(),
                        playlists = results.playlists.len(),
                        "Search completed successfully"
                    );
                    // Mark tracks with liked status from cache
                    spotify.mark_tracks_liked(&mut results.tracks).await;
                    model.set_search_results(results).await;
                    // Switch to MainContent section to show results
                    let mut ui_state = model.ui_state.lock().await;
                    ui_state.active_section = ActiveSection::MainContent;
                }
                Err(e) => {
                    error!(query, error = %e, "Search failed");
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    async fn handle_selected_item(&self, item: SelectedItem) {
        match item {
            SelectedItem::Track { uri, .. } => {
                // Ensure we have a device to play on
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
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
                // Ensure we have a device to play on
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
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
                // Ensure we have a device to play on
                if !self.ensure_device_available().await {
                    return;
                }

                let model = self.model.lock().await;
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
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_album(&id).await {
                        Ok(mut detail) => {
                            // Mark tracks with liked status from cache
                            spotify.mark_tracks_liked(&mut detail.tracks).await;
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
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_artist(&id).await {
                        Ok(mut detail) => {
                            // Mark tracks with liked status from cache
                            spotify.mark_tracks_liked(&mut detail.top_tracks).await;
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
                let model = self.model.lock().await;
                model.set_content_loading(true).await;
                if let Some(spotify) = &model.spotify {
                    match spotify.get_playlist(&id).await {
                        Ok(mut detail) => {
                            // Mark tracks with liked status from cache
                            spotify.mark_tracks_liked(&mut detail.tracks).await;
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
        let is_playing = model.is_playing().await;
        debug!(is_playing, "Toggling playback");

        // If we're about to play (not pause), ensure we have a device
        if !is_playing {
            drop(model);
            if !self.ensure_device_available().await {
                return;
            }
        } else {
            drop(model);
        }

        let model = self.model.lock().await;
        if let Some(spotify) = &model.spotify {
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
                error!(error = %e, "Toggle playback failed");
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                info!(action = if is_playing { "paused" } else { "resumed" }, "Playback toggled");
            }
            // Note: State will be updated via player events, no need to poll
        }
    }

    async fn next_track(&self) {
        debug!("Skipping to next track");
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                async move { spotify.next_track().await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                error!(error = %e, "Next track failed");
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                info!("Skipped to next track");
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

    /// Open the device picker modal
    async fn open_device_picker(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_available_devices().await {
                Ok(devices) => {
                    if devices.is_empty() {
                        model.set_error("No devices available".to_string()).await;
                    } else {
                        model.show_device_picker(devices).await;
                    }
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    /// Select a device for playback
    async fn select_device(&self, device: &crate::model::DeviceInfo, local_device_name: &str) {
        let is_local_device = device.name == local_device_name;
        info!(device_name = %device.name, device_id = %device.id, is_local_device, "Selecting playback device");

        if is_local_device {
            // Wait for backend to be ready (up to 5 seconds)
            for _ in 0..50 {
                let backend_guard = self.audio_backend.lock().await;
                if backend_guard.is_some() {
                    break;
                }
                drop(backend_guard);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Activate the local audio backend
            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                if let Err(e) = backend.activate().await {
                    error!(error = %e, "Failed to activate local audio backend");
                    drop(backend_guard);
                    let model = self.model.lock().await;
                    model.set_error(format!("Failed to activate audio: {}", e)).await;
                    return;
                }
            } else {
                warn!("Audio backend not ready when trying to select local device");
                drop(backend_guard);
                let model = self.model.lock().await;
                model.set_error("Audio backend not ready".to_string()).await;
                return;
            }
            drop(backend_guard);

            // Try to start event listener if not already started
            self.try_start_event_listener().await;

            // Give the device a moment to register with Spotify
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        } else {
            // If switching away from local device, stop playback on it
            debug!("Switching away from local device, stopping local playback");
            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                backend.stop().await;
            }
        }

        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            // Transfer playback to the selected device
            match spotify.transfer_playback_to_device(&device.id, true).await {
                Ok(()) => {
                    info!(device_name = %device.name, "Playback transferred successfully");
                    // Update the displayed device name
                    model.update_device_name(device.name.clone()).await;
                }
                Err(e) => {
                    error!(device_name = %device.name, error = %e, "Failed to transfer playback");
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }

        self.refresh_playback().await;
    }

    /// Ensure there's an active device available for playback
    /// If no device is active, activates the local audio backend
    /// Returns true if a device is available
    pub async fn ensure_device_available(&self) -> bool {
        debug!("Checking for available playback device");
        // First check if our local backend is already active
        {
            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                if backend.is_active().await {
                    debug!("Local backend already active");
                    return true;
                }
            }
        }

        // Check if there's an active device on Spotify
        {
            let model = self.model.lock().await;
            if let Some(spotify) = &model.spotify {
                if spotify.has_active_device().await {
                    debug!("Found active device on Spotify");
                    return true;
                }
            }
        }

        debug!("No active device found, activating local backend");
        // No active device - wait for backend to be ready (up to 5 seconds)
        for _ in 0..50 {
            let backend_guard = self.audio_backend.lock().await;
            if backend_guard.is_some() {
                break;
            }
            drop(backend_guard);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Now try to activate our local backend
        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            match backend.activate().await {
                Ok(()) => {
                    drop(backend_guard);
                    
                    // Try to start event listener
                    self.try_start_event_listener().await;
                    
                    // Update device name
                    let model = self.model.lock().await;
                    let local_device_name = AudioBackend::get_device_name().to_string();
                    model.update_device_name(local_device_name.clone()).await;

                    // Give it time to register with Spotify Connect
                    drop(model);
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    // Transfer playback to our local device to make it active
                    let model = self.model.lock().await;
                    if let Some(spotify) = &model.spotify {
                        // Find our device by name
                        if let Ok(devices) = spotify.get_available_devices().await {
                            if let Some(our_device) = devices.iter().find(|d| d.name == local_device_name) {
                                info!(device_name = %local_device_name, device_id = %our_device.id, "Transferring playback to local device");
                                // Transfer playback to make our device active (don't start playing yet)
                                if let Err(e) = spotify.transfer_playback_to_device(&our_device.id, false).await {
                                    warn!(error = %e, "Failed to transfer playback to local device, will try during play");
                                }
                            } else {
                                warn!(device_name = %local_device_name, "Local device not found in available devices list");
                                debug!(?devices, "Available devices");
                            }
                        }
                    }

                    info!("Local audio backend activated for playback");
                    true
                }
                Err(e) => {
                    error!(error = %e, "Failed to activate local audio backend");
                    drop(backend_guard);
                    let model = self.model.lock().await;
                    model.set_error(format!("Failed to activate audio: {}", e)).await;
                    false
                }
            }
        } else {
            warn!("Audio backend not ready after waiting");
            // Backend still not ready after waiting
            let model = self.model.lock().await;
            model.set_error("Audio backend not ready. Please try again.".to_string()).await;
            false
        }
    }

    /// Initialize playback state on startup
    /// - If another device is playing: show that device's info, control it
    /// - If no device is playing: activate local device as default
    /// Initialize playback state on startup
    /// - If another device is playing: show that device's info, control it
    /// - If no device is playing: activate local device as default
    pub async fn initialize_playback(&self) {
        // First, try to start the event listener if backend is ready
        self.try_start_event_listener().await;
        
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_current_playback().await {
                Ok(Some(playback)) => {
                    // There's active playback on another device - use that
                    if playback.is_playing {
                        // Device is actually playing - use it
                        model.update_device_name(playback.device.name.clone()).await;
                        model.update_from_playback_context(&playback).await;
                        // Don't activate local backend - just control the existing device
                        return;
                    }
                    // Device exists but not playing - fall through to activate local
                }
                Ok(None) => {
                    // No active playback - will activate local device below
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                    // Still try to activate local device on error
                }
            }
        }

        // No active playback - activate local device as default
        // Wait for backend to be ready (up to 10 seconds)
        drop(model);

        for _ in 0..100 {
            let backend_guard = self.audio_backend.lock().await;
            if backend_guard.is_some() {
                break;
            }
            drop(backend_guard);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            if backend.activate().await.is_ok() {
                drop(backend_guard);
                self.try_start_event_listener().await;
                let model = self.model.lock().await;
                model.update_device_name(AudioBackend::get_device_name().to_string()).await;

                // Give it a moment to register with Spotify
                drop(model);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        } else {
            let model = self.model.lock().await;
            model.set_error("Audio backend failed to initialize".to_string()).await;
        }
    }

    /// Refresh playback state from Spotify API
    pub async fn refresh_playback(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_current_playback().await {
                Ok(Some(playback)) => {
                    // There's active playback - update from it
                    model.update_device_name(playback.device.name.clone()).await;
                    model.update_from_playback_context(&playback).await;
                }
                Ok(None) => {
                    // No active playback
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
                Ok(mut detail) => {
                    // Mark tracks with liked status from cache
                    spotify.mark_tracks_liked(&mut detail.tracks).await;
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
                        Ok(mut tracks) => {
                            // Mark tracks with liked status from cache
                            spotify.mark_tracks_liked(&mut tracks).await;
                            model.set_recently_played(tracks).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                1 => {
                    // Liked songs (already marked as liked in get_liked_songs)
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

        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            match backend.restart().await {
                Ok(event_channel) => {
                    drop(backend_guard);
                    // Wait a bit for device registration with Spotify
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    {
                        let model = self.model.lock().await;
                        model.clear_error().await;
                    }
                    Some(event_channel)
                }
                Err(e) => {
                    drop(backend_guard);
                    let model = self.model.lock().await;
                    model.set_error(format!("Audio reconnect failed: {}", e)).await;
                    None
                }
            }
        } else {
            let model = self.model.lock().await;
            model.set_error("Audio backend not initialized".to_string()).await;
            None
        }
    }

    /// Execute a playback operation with automatic backend recovery on failure
    /// Only attempts to restart the local audio backend if it was already initialized
    async fn with_backend_recovery<F, Fut>(&self, operation: F) -> Result<()>
    where
        F: Fn() -> Fut + Clone,
        Fut: Future<Output = Result<()>>,
    {
        // First attempt
        match operation().await {
            Ok(()) => Ok(()),
            Err(e) => {
                debug!(error = %e, "Playback operation failed, checking if recovery is needed");

                // Check if this is a device unavailable error and the local backend exists
                let local_backend_exists = {
                    let backend_guard = self.audio_backend.lock().await;
                    backend_guard.is_some()
                };

                let is_device_error = Self::is_device_unavailable_error(&e);
                debug!(is_device_error, local_backend_exists, "Recovery check");

                if is_device_error && local_backend_exists {
                    info!("Device unavailable error detected, attempting backend recovery");
                    // Try to restart the local backend
                    if let Some(event_channel) = self.try_restart_audio_backend().await {
                        // Start listening to events from the new backend
                        self.start_player_event_listener(event_channel);

                        // Wait a bit more for stability
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Retry the operation
                        debug!("Retrying playback operation after backend recovery");
                        return match operation().await {
                            Ok(()) => {
                                info!("Playback operation succeeded after recovery");
                                Ok(())
                            }
                            Err(retry_err) => {
                                error!(error = %retry_err, "Playback operation failed even after recovery");
                                Err(retry_err)
                            }
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// Start listening to librespot player events for real-time playback updates
    pub fn start_player_event_listener(&self, mut event_channel: PlayerEventChannel) {
        let model = self.model.clone();
        info!("Starting librespot player event listener");

        tokio::spawn(async move {
            while let Some(event) = event_channel.recv().await {
                let model_guard = model.lock().await;

                if model_guard.should_quit().await {
                    debug!("Player event listener shutting down");
                    break;
                }

                match event {
                    PlayerEvent::Playing { position_ms, .. } => {
                        trace!(position_ms, "PlayerEvent::Playing");
                        model_guard.update_playback_position(position_ms, true).await;
                    }
                    PlayerEvent::Paused { position_ms, .. } => {
                        debug!(position_ms, "PlayerEvent::Paused");
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::PositionChanged { position_ms, .. } => {
                        // Periodic position update - keep current playing state
                        trace!(position_ms, "PlayerEvent::PositionChanged");
                        let is_playing = model_guard.is_playing().await;
                        model_guard.update_playback_position(position_ms, is_playing).await;
                    }
                    PlayerEvent::Seeked { position_ms, .. } => {
                        debug!(position_ms, "PlayerEvent::Seeked");
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

                        info!(
                            track = %audio_item.name,
                            artist = %artist,
                            album = %album,
                            duration_ms = audio_item.duration_ms,
                            "PlayerEvent::TrackChanged"
                        );

                        let track = TrackMetadata {
                            name: audio_item.name.clone(),
                            artist,
                            album,
                            duration_ms: audio_item.duration_ms,
                        };
                        model_guard.update_track_info(track).await;
                    }
                    PlayerEvent::Stopped { .. } => {
                        debug!("PlayerEvent::Stopped");
                        model_guard.update_playback_position(0, false).await;
                    }
                    PlayerEvent::Loading { position_ms, .. } => {
                        // Track is loading, update position
                        debug!(position_ms, "PlayerEvent::Loading");
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::EndOfTrack { .. } => {
                        // Track ended, will transition to next track
                        debug!("PlayerEvent::EndOfTrack");
                        model_guard.set_playing(false).await;
                    }
                    _ => {
                        // Ignore other events (volume, session, shuffle, repeat, etc.)
                        trace!("PlayerEvent: other event received");
                    }
                }
            }
        });
    }
}
