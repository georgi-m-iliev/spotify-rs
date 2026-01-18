use anyhow::Result;
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem},
    prelude::*,
    AuthCodeSpotify,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SpotifyClient {
    client: Arc<AuthCodeSpotify>,
    device_name: Option<String>,
}

impl SpotifyClient {
    pub fn new(client: AuthCodeSpotify, device_name: Option<String>) -> Self {
        Self {
            client: Arc::new(client),
            device_name,
        }
    }

    pub async fn get_current_playback(&self) -> Result<Option<CurrentPlaybackContext>> {
        Ok(self.client.current_playback(None, None::<Vec<_>>).await?)
    }

    async fn get_device_id(&self) -> Option<String> {
        if let Ok(devices) = self.client.device().await {
            // First try to find our librespot device
            if let Some(ref name) = self.device_name {
                if let Some(device) = devices.iter().find(|d| d.name == *name) {
                    return device.id.clone();
                }
            }
            // Fall back to active device
            devices.into_iter().find(|d| d.is_active).and_then(|d| d.id)
        } else {
            None
        }
    }

    pub async fn play(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client
            .resume_playback(device_id.as_deref(), None)
            .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.pause_playback(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn next_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.next_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn previous_track(&self) -> Result<()> {
        let device_id = self.get_device_id().await;
        self.client.previous_track(device_id.as_deref()).await?;
        Ok(())
    }

    pub async fn transfer_playback(&self) -> Result<()> {
        if let Some(device_id) = self.get_device_id().await {
            self.client
                .transfer_playback(&device_id, Some(true))
                .await?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub progress_ms: u32,
}

impl Default for TrackInfo {
    fn default() -> Self {
        Self {
            name: "No track playing".to_string(),
            artist: "".to_string(),
            album: "".to_string(),
            duration_ms: 0,
            progress_ms: 0,
        }
    }
}

impl TrackInfo {
    pub fn from_playback(playback: &CurrentPlaybackContext) -> Self {
        if let Some(item) = &playback.item {
            match item {
                PlayableItem::Track(track) => {
                    let artist = track
                        .artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();

                    let album = track.album.name.clone();

                    Self {
                        name: track.name.clone(),
                        artist,
                        album,
                        duration_ms: track.duration.num_milliseconds() as u32,
                        progress_ms: playback
                            .progress
                            .map(|d| d.num_milliseconds() as u32)
                            .unwrap_or(0),
                    }
                }
                PlayableItem::Episode(episode) => Self {
                    name: episode.name.clone(),
                    artist: episode.show.name.clone(),
                    album: "Podcast".to_string(),
                    duration_ms: episode.duration.num_milliseconds() as u32,
                    progress_ms: playback
                        .progress
                        .map(|d| d.num_milliseconds() as u32)
                        .unwrap_or(0),
                },
                PlayableItem::Unknown(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }
}

/// Tracks playback state with local time-based progress calculation
#[derive(Clone)]
pub struct PlaybackState {
    /// Position in milliseconds at the time of the last update
    pub position_ms: u32,
    /// When the position was last updated
    pub last_update: Instant,
    /// Whether playback is currently active
    pub is_playing: bool,
    /// Duration of the current track in milliseconds
    pub duration_ms: u32,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position_ms: 0,
            last_update: Instant::now(),
            is_playing: false,
            duration_ms: 0,
        }
    }
}

impl PlaybackState {
    /// Calculate the current position based on elapsed time since last update
    pub fn current_position_ms(&self) -> u32 {
        if self.is_playing && self.duration_ms > 0 {
            let elapsed = self.last_update.elapsed().as_millis() as u32;
            (self.position_ms.saturating_add(elapsed)).min(self.duration_ms)
        } else {
            self.position_ms.min(self.duration_ms.max(1) - 1)
        }
    }

    /// Update position, only accepting updates that make sense
    /// (prevents backwards jumps from network timing issues)
    pub fn update_position(&mut self, new_position_ms: u32, is_playing: bool) {
        let current_calculated = self.current_position_ms();

        // Calculate how far off the new position is from our calculated position
        let diff = new_position_ms as i64 - current_calculated as i64;

        // Always accept if:
        // 1. Play state changed (pause/resume)
        // 2. Significant backward jump (likely a seek) - more than 2 seconds back
        // 3. Significant forward jump (likely a seek) - more than 2 seconds ahead
        // 4. We were paused (no local time tracking was happening)
        // 5. New position is within reasonable range and ahead (normal sync)
        let state_changed = self.is_playing != is_playing;
        let significant_backward_jump = diff < -2000;
        let significant_forward_jump = diff > 2000;
        let was_paused = !self.is_playing;

        // For small differences, only accept if moving forward or very close
        // This prevents the "back and forth" issue while still allowing correction
        let acceptable_sync = diff >= -100; // Allow up to 100ms backward for timing jitter

        if state_changed || significant_backward_jump || significant_forward_jump || was_paused || acceptable_sync {
            self.position_ms = new_position_ms;
            self.last_update = Instant::now();
        }
        // Always update play state
        self.is_playing = is_playing;
    }
}

pub struct AppModel {
    pub spotify: Option<SpotifyClient>,
    pub current_track: Arc<Mutex<TrackInfo>>,
    pub playback_state: Arc<Mutex<PlaybackState>>,
    pub should_quit: Arc<Mutex<bool>>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            spotify: None,
            current_track: Arc::new(Mutex::new(TrackInfo::default())),
            playback_state: Arc::new(Mutex::new(PlaybackState::default())),
            should_quit: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_spotify_client(&mut self, client: SpotifyClient) {
        self.spotify = Some(client);
    }

    /// Update track metadata (name, artist, album, duration)
    pub async fn update_track_info(&self, track: TrackInfo) {
        let duration_ms = track.duration_ms;
        *self.current_track.lock().await = track;

        // Also update duration in playback state
        let mut state = self.playback_state.lock().await;
        state.duration_ms = duration_ms;
    }

    /// Update playback position and playing state from librespot events
    pub async fn update_playback_position(&self, position_ms: u32, is_playing: bool) {
        let mut state = self.playback_state.lock().await;
        state.update_position(position_ms, is_playing);
    }

    /// Update just the playing/paused state
    pub async fn set_playing(&self, is_playing: bool) {
        let mut state = self.playback_state.lock().await;
        // When pausing/resuming, update the position to current calculated value
        state.position_ms = state.current_position_ms();
        state.is_playing = is_playing;
        state.last_update = Instant::now();
    }

    /// Legacy method for backward compatibility with rspotify polling
    pub async fn update_playback_state(&self, track: TrackInfo, is_playing: bool) {
        let duration_ms = track.duration_ms;
        let progress_ms = track.progress_ms;
        *self.current_track.lock().await = track;

        let mut state = self.playback_state.lock().await;
        // For initial sync from API, always accept the position
        state.position_ms = progress_ms;
        state.duration_ms = duration_ms;
        state.is_playing = is_playing;
        state.last_update = Instant::now();
    }

    /// Get track info with current calculated progress
    pub async fn get_track_info(&self) -> TrackInfo {
        let mut track = self.current_track.lock().await.clone();
        let state = self.playback_state.lock().await;
        track.progress_ms = state.current_position_ms();
        track
    }

    pub async fn is_playing(&self) -> bool {
        self.playback_state.lock().await.is_playing
    }

    pub async fn should_quit(&self) -> bool {
        *self.should_quit.lock().await
    }

    pub async fn set_should_quit(&self, quit: bool) {
        *self.should_quit.lock().await = quit;
    }
}
