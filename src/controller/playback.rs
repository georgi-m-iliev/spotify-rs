//! Playback control methods

use anyhow::Result;
use std::future::Future;

use crate::audio::AudioBackend;
use crate::model::{ActiveSection, RepeatState};

use super::AppController;

impl AppController {
    pub async fn toggle_playback(&self) {
        let model = self.model.lock().await;
        let is_playing = model.is_playing().await;
        tracing::debug!(is_playing, "Toggling playback");

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

            drop(model);

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
                tracing::error!(error = %e, "Toggle playback failed");
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                tracing::info!(action = if is_playing { "paused" } else { "resumed" }, "Playback toggled");
            }
        }
    }

    pub async fn next_track(&self) {
        tracing::debug!("Skipping to next track");
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let spotify_clone = spotify.clone();
            drop(model);

            let operation = move || {
                let spotify = spotify_clone.clone();
                async move { spotify.next_track().await }
            };

            if let Err(e) = self.with_backend_recovery(operation).await {
                tracing::error!(error = %e, "Next track failed");
                let model = self.model.lock().await;
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                tracing::info!("Skipped to next track");
            }
        }
    }

    pub async fn previous_track(&self) {
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
    }

    pub async fn toggle_shuffle(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if !spotify.has_active_device().await {
                model.set_error("No active playback. Start playing a song first.".to_string()).await;
                return;
            }

            let current_shuffle = model.get_shuffle_state().await;
            let new_shuffle = !current_shuffle;

            if let Err(e) = spotify.set_shuffle(new_shuffle).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                model.set_shuffle(new_shuffle).await;
                drop(model);
                // delay is needed because Spotify API needs to propagate the change
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                self.refresh_queue_if_visible().await;
            }
        }
    }

    pub async fn cycle_repeat(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if !spotify.has_active_device().await {
                model.set_error("No active playback. Start playing a song first.".to_string()).await;
                return;
            }

            let current_repeat = model.get_repeat_state().await;
            let new_repeat = match current_repeat {
                RepeatState::Off => RepeatState::All,
                RepeatState::All => RepeatState::One,
                RepeatState::One => RepeatState::Off,
            };

            if let Err(e) = spotify.set_repeat(new_repeat).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                model.set_repeat(new_repeat).await;
            }
        }
    }

    pub async fn volume_up(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_volume = model.get_volume().await;
            let new_volume = (current_volume + 5).min(100);

            if let Err(e) = spotify.set_volume(new_volume).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                model.set_volume(new_volume).await;
            }
        }
    }

    pub async fn volume_down(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let current_volume = model.get_volume().await;
            let new_volume = current_volume.saturating_sub(5);

            if let Err(e) = spotify.set_volume(new_volume).await {
                let error_msg = Self::format_error(&e);
                model.set_error(error_msg).await;
            } else {
                model.set_volume(new_volume).await;
            }
        }
    }

    pub async fn toggle_liked_track(&self, track_id: &str) {
        if track_id.is_empty() {
            tracing::warn!("Cannot toggle liked status: track ID is empty");
            let model = self.model.lock().await;
            model.set_error("Cannot like/unlike: track has no ID".to_string()).await;
            return;
        }

        tracing::debug!(track_id, "Toggling liked status for track");

        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.toggle_liked_song(track_id).await {
                Ok(new_liked_status) => {
                    model.update_track_liked_status(track_id, new_liked_status).await;

                    let status = if new_liked_status { "added to" } else { "removed from" };
                    tracing::info!(track_id, status, "Track liked status toggled");
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn show_queue(&self) {
        let model = self.model.lock().await;
        model.set_content_loading(true).await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_queue().await {
                Ok((currently_playing, queue)) => {
                    let mut filtered_queue = Vec::with_capacity(queue.len());
                    for track in queue {
                        if !model.is_in_queue_skip_list(&track.uri).await {
                            filtered_queue.push(track);
                        }
                    }

                    model.set_queue(currently_playing, filtered_queue).await;
                    model.set_active_section(ActiveSection::MainContent).await;
                }
                Err(e) => {
                    model.set_content_loading(false).await;
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn refresh_queue_if_visible(&self) {
        let model = self.model.lock().await;

        if !model.is_queue_view_visible().await {
            return;
        }

        if let Some(spotify) = &model.spotify {
            match spotify.get_queue().await {
                Ok((currently_playing, queue)) => {
                    let mut filtered_queue = Vec::with_capacity(queue.len());
                    for track in queue {
                        if !model.is_in_queue_skip_list(&track.uri).await {
                            filtered_queue.push(track);
                        }
                    }

                    model.update_queue_if_visible(currently_playing, filtered_queue).await;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "Failed to refresh queue");
                }
            }
        }
    }

    pub async fn add_track_to_queue(&self, track_uri: &str) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.add_to_queue(track_uri).await {
                Ok(()) => {
                    tracing::info!(track_uri, "Track added to queue");
                }
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn open_device_picker(&self) {
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

    pub async fn select_device(&self, device: &crate::model::DeviceInfo, local_device_name: &str) {
        let is_local_device = device.name == local_device_name;
        tracing::info!(device_name = %device.name, device_id = %device.id, is_local_device, "Selecting playback device");

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

            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                if let Err(e) = backend.activate().await {
                    tracing::error!(error = %e, "Failed to activate local audio backend");
                    drop(backend_guard);
                    let model = self.model.lock().await;
                    model.set_error(format!("Failed to activate audio: {}", e)).await;
                    return;
                }
            } else {
                tracing::warn!("Audio backend not ready when trying to select local device");
                drop(backend_guard);
                let model = self.model.lock().await;
                model.set_error("Audio backend not ready".to_string()).await;
                return;
            }
            drop(backend_guard);

            self.try_start_event_listener().await;

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        } else {
            tracing::debug!("Switching away from local device, stopping local playback");
            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                backend.stop().await;
            }
        }

        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.transfer_playback_to_device(&device.id, true).await {
                Ok(()) => {
                    tracing::info!(device_name = %device.name, "Playback transferred successfully");
                    model.update_device_name(device.name.clone()).await;
                }
                Err(e) => {
                    tracing::error!(device_name = %device.name, error = %e, "Failed to transfer playback");
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }

        self.refresh_playback().await;
    }

    pub async fn ensure_device_available(&self) -> bool {
        tracing::debug!("Checking for available playback device");
        // First check if our local backend is already active
        {
            let backend_guard = self.audio_backend.lock().await;
            if let Some(backend) = backend_guard.as_ref() {
                if backend.is_active().await {
                    tracing::debug!("Local backend already active");
                    return true;
                }
            }
        }

        // Check if there's an active device on Spotify
        {
            let model = self.model.lock().await;
            if let Some(spotify) = &model.spotify {
                if spotify.has_active_device().await {
                    tracing::debug!("Found active device on Spotify");
                    return true;
                }
            }
        }

        tracing::debug!("No active device found, activating local backend");
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
                    
                    self.try_start_event_listener().await;
                    
                    let model = self.model.lock().await;
                    let local_device_name = AudioBackend::get_device_name().to_string();
                    model.update_device_name(local_device_name.clone()).await;

                    drop(model);
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    // Transfer playback to our local device to make it active
                    let model = self.model.lock().await;
                    if let Some(spotify) = &model.spotify {
                        if let Ok(devices) = spotify.get_available_devices().await {
                            if let Some(our_device) = devices.iter().find(|d| d.name == local_device_name) {
                                tracing::info!(device_name = %local_device_name, device_id = %our_device.id, "Transferring playback to local device");
                                if let Err(e) = spotify.transfer_playback_to_device(&our_device.id, false).await {
                                    tracing::warn!(error = %e, "Failed to transfer playback to local device, will try during play");
                                }
                            } else {
                                tracing::warn!(device_name = %local_device_name, "Local device not found in available devices list");
                                tracing::debug!(?devices, "Available devices");
                            }
                        }
                    }

                    tracing::info!("Local audio backend activated for playback");
                    true
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to activate local audio backend");
                    drop(backend_guard);
                    let model = self.model.lock().await;
                    model.set_error(format!("Failed to activate audio: {}", e)).await;
                    false
                }
            }
        } else {
            tracing::warn!("Audio backend not ready after waiting");
            let model = self.model.lock().await;
            model.set_error("Audio backend not ready. Please try again.".to_string()).await;
            false
        }
    }

    pub async fn initialize_playback(&self) {
        self.try_start_event_listener().await;
        
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_current_playback().await {
                Ok(Some(playback)) => {
                    // There's active playback on another device - use that
                    if playback.is_playing {
                        model.update_device_name(playback.device.name.clone()).await;
                        model.update_from_playback_context(&playback).await;
                        return;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }

        // No active playback - activate local device as default
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

                drop(model);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        } else {
            let model = self.model.lock().await;
            model.set_error("Audio backend failed to initialize".to_string()).await;
        }
    }

    pub async fn refresh_playback(&self) {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            match spotify.get_current_playback().await {
                Ok(Some(playback)) => {
                    model.update_device_name(playback.device.name.clone()).await;
                    model.update_from_playback_context(&playback).await;
                }
                Ok(None) => {}
                Err(e) => {
                    let error_msg = Self::format_error(&e);
                    model.set_error(error_msg).await;
                }
            }
        }
    }

    pub async fn with_backend_recovery<F, Fut>(&self, operation: F) -> Result<()>
    where
        F: Fn() -> Fut + Clone,
        Fut: Future<Output = Result<()>>,
    {
        // First attempt
        match operation().await {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::debug!(error = %e, "Playback operation failed, checking if recovery is needed");

                // Check if this is a device unavailable error and the local backend exists
                let local_backend_exists = {
                    let backend_guard = self.audio_backend.lock().await;
                    backend_guard.is_some()
                };

                let is_device_error = Self::is_device_unavailable_error(&e);
                tracing::debug!(is_device_error, local_backend_exists, "Recovery check");

                if is_device_error && local_backend_exists {
                    tracing::info!("Device unavailable error detected, attempting backend recovery");
                    // Try to restart the local backend
                    if let Some(event_channel) = self.try_restart_audio_backend().await {
                        // Start listening to events from the new backend
                        let audio_backend = self.audio_backend.clone();
                        self.start_player_event_listener(event_channel, audio_backend);

                        // Wait a bit more for stability
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Retry the operation
                        tracing::debug!("Retrying playback operation after backend recovery");
                        return match operation().await {
                            Ok(()) => {
                                tracing::info!("Playback operation succeeded after recovery");
                                Ok(())
                            }
                            Err(retry_err) => {
                                tracing::error!(error = %retry_err, "Playback operation failed even after recovery");
                                Err(retry_err)
                            }
                        }
                    }
                }
                Err(e)
            }
        }
    }

    fn is_device_unavailable_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("404")
            || error_str.contains("no active device")
            || error_str.contains("device not found")
            || error_str.contains("player command failed")
    }

    pub async fn try_restart_audio_backend(&self) -> Option<librespot::playback::player::PlayerEventChannel> {
        {
            let model = self.model.lock().await;
            model.set_error("Reconnecting audio...".to_string()).await;
        }

        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            match backend.restart().await {
                Ok(event_channel) => {
                    drop(backend_guard);
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
}
