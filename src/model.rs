use anyhow::Result;
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem},
    prelude::*,
    AuthCodeSpotify,
};
use std::sync::Arc;
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

pub struct AppModel {
    pub spotify: Option<SpotifyClient>,
    pub current_track: Arc<Mutex<TrackInfo>>,
    pub is_playing: Arc<Mutex<bool>>,
    pub should_quit: Arc<Mutex<bool>>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            spotify: None,
            current_track: Arc::new(Mutex::new(TrackInfo::default())),
            is_playing: Arc::new(Mutex::new(false)),
            should_quit: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_spotify_client(&mut self, client: SpotifyClient) {
        self.spotify = Some(client);
    }

    pub async fn update_playback_state(&self, track: TrackInfo, is_playing: bool) {
        *self.current_track.lock().await = track;
        *self.is_playing.lock().await = is_playing;
    }

    pub async fn get_track_info(&self) -> TrackInfo {
        self.current_track.lock().await.clone()
    }

    pub async fn is_playing(&self) -> bool {
        *self.is_playing.lock().await
    }

    pub async fn should_quit(&self) -> bool {
        *self.should_quit.lock().await
    }

    pub async fn set_should_quit(&self, quit: bool) {
        *self.should_quit.lock().await = quit;
    }
}
