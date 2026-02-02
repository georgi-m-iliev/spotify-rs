//! Player event listener for librespot playback events

use std::sync::Arc;
use tokio::sync::Mutex;
use librespot::metadata::audio::UniqueFields;
use librespot::playback::player::{PlayerEvent, PlayerEventChannel};

use crate::audio::AudioBackend;
use crate::model::TrackMetadata;
use super::AppController;

impl AppController {
    pub fn start_player_event_listener(
        &self,
        mut event_channel: PlayerEventChannel,
        audio_backend: Arc<Mutex<Option<AudioBackend>>>,
    ) {
        let model = self.model.clone();
        let controller = self.clone();
        tracing::info!("Starting librespot player event listener");

        tokio::spawn(async move {
            while let Some(event) = event_channel.recv().await {
                let model_guard = model.lock().await;

                if model_guard.should_quit().await {
                    tracing::debug!("Player event listener shutting down");
                    break;
                }

                match event {
                    PlayerEvent::Playing { position_ms, .. } => {
                        tracing::trace!(position_ms, "PlayerEvent::Playing");
                        model_guard.update_playback_position(position_ms, true).await;
                    }
                    PlayerEvent::Paused { position_ms, .. } => {
                        tracing::debug!(position_ms, "PlayerEvent::Paused");
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::PositionChanged { position_ms, .. } => {
                        tracing::trace!(position_ms, "PlayerEvent::PositionChanged");
                        let is_playing = model_guard.is_playing().await;
                        model_guard.update_playback_position(position_ms, is_playing).await;
                    }
                    PlayerEvent::Seeked { position_ms, .. } => {
                        tracing::debug!(position_ms, "PlayerEvent::Seeked");
                        let is_playing = model_guard.is_playing().await;
                        model_guard.update_playback_position(position_ms, is_playing).await;
                    }
                    PlayerEvent::TrackChanged { audio_item } => {
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

                        // Build URI from track_id - to_uri() returns the full "spotify:track:xxx" format
                        let uri = audio_item.track_id.to_uri().unwrap_or_default();

                        if model_guard.is_in_queue_skip_list(&uri).await {
                            tracing::info!(
                                track = %audio_item.name,
                                uri = %uri,
                                "Track is in skip list, auto-skipping"
                            );
                            model_guard.remove_from_queue_skip_list(&uri).await;

                            drop(model_guard);

                            let backend_guard = audio_backend.lock().await;
                            if let Some(backend) = backend_guard.as_ref() {
                                if let Err(e) = backend.skip_to_next().await {
                                    tracing::error!(error = %e, "Failed to auto-skip track");
                                }
                            }
                            continue;
                        }

                        tracing::info!(
                            track = %audio_item.name,
                            artist = %artist,
                            album = %album,
                            duration_ms = audio_item.duration_ms,
                            uri = %uri,
                            "PlayerEvent::TrackChanged"
                        );

                        let track = TrackMetadata {
                            name: audio_item.name.clone(),
                            artist,
                            album,
                            duration_ms: audio_item.duration_ms,
                            uri,
                        };
                        model_guard.update_track_info(track).await;

                        drop(model_guard);
                        controller.refresh_queue_if_visible().await;
                        continue;
                    }
                    PlayerEvent::Stopped { .. } => {
                        tracing::debug!("PlayerEvent::Stopped");
                        model_guard.update_playback_position(0, false).await;
                    }
                    PlayerEvent::Loading { position_ms, .. } => {
                        tracing::debug!(position_ms, "PlayerEvent::Loading");
                        model_guard.update_playback_position(position_ms, false).await;
                    }
                    PlayerEvent::EndOfTrack { .. } => {
                        tracing::debug!("PlayerEvent::EndOfTrack");
                        model_guard.set_playing(false).await;
                    }
                    _ => {
                        tracing::trace!("PlayerEvent: other event received");
                    }
                }
            }
        });
    }
}
