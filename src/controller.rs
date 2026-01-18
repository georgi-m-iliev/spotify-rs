use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use librespot::metadata::audio::UniqueFields;
use librespot::playback::player::{PlayerEvent, PlayerEventChannel};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::model::{AppModel, TrackInfo};

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

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                let model = self.model.lock().await;
                model.set_should_quit(true).await;
            }
            KeyCode::Char(' ') => {
                self.toggle_playback().await?;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.next_track().await?;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.previous_track().await?;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_playback().await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn toggle_playback(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            let is_playing = model.is_playing().await;

            if is_playing {
                spotify.pause().await?;
            } else {
                spotify.play().await?;
            }
            // Note: State will be updated via player events, no need to poll
        }

        Ok(())
    }

    async fn next_track(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            spotify.next_track().await?;
        }
        // Note: State will be updated via player events

        Ok(())
    }

    async fn previous_track(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            spotify.previous_track().await?;
        }
        // Note: State will be updated via player events

        Ok(())
    }

    /// Refresh playback state from Spotify API (fallback/initial sync)
    pub async fn refresh_playback(&self) -> Result<()> {
        let model = self.model.lock().await;

        if let Some(spotify) = &model.spotify {
            if let Some(playback) = spotify.get_current_playback().await? {
                let track = TrackInfo::from_playback(&playback);
                let is_playing = playback.is_playing;
                model.update_playback_state(track, is_playing).await;
            }
        }

        Ok(())
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

                        let track = TrackInfo {
                            name: audio_item.name.clone(),
                            artist,
                            album,
                            duration_ms: audio_item.duration_ms,
                            progress_ms: 0,
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
