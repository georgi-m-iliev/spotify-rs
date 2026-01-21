use std::sync::Arc;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::Mutex;
use rspotify::{
    model::{CurrentPlaybackContext, PlayableItem},
    prelude::*,
    AuthCodeSpotify,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveSection {
    Search,
    Library,
    Playlists,
    MainContent,
}

impl ActiveSection {
    pub fn next(self) -> Self {
        match self {
            ActiveSection::Search => ActiveSection::Library,
            ActiveSection::Library => ActiveSection::Playlists,
            ActiveSection::Playlists => ActiveSection::MainContent,
            ActiveSection::MainContent => ActiveSection::Search,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ActiveSection::Search => ActiveSection::MainContent,
            ActiveSection::Library => ActiveSection::Search,
            ActiveSection::Playlists => ActiveSection::Library,
            ActiveSection::MainContent => ActiveSection::Playlists,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LibraryItem {
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct PlaylistItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct UiState {
    pub active_section: ActiveSection,
    pub search_query: String,
    pub library_items: Vec<LibraryItem>,
    pub library_selected: usize,
    pub playlists: Vec<PlaylistItem>,
    pub playlist_selected: usize,
    pub error_message: Option<String>,
    pub error_timestamp: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatState {
    Off,
    All,
    One,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_section: ActiveSection::Search,
            search_query: String::new(),
            library_items: vec![
                LibraryItem { name: "Made for you".to_string() },
                LibraryItem { name: "Recently played".to_string() },
                LibraryItem { name: "Liked songs".to_string() },
                LibraryItem { name: "Albums".to_string() },
                LibraryItem { name: "Artists".to_string() },
            ],
            library_selected: 0,
            playlists: vec![
                PlaylistItem { id: "1".to_string(), name: "Playlist Example 1".to_string() },
                PlaylistItem { id: "2".to_string(), name: "Playlist Example 2".to_string() },
                PlaylistItem { id: "3".to_string(), name: "Playlist Example 3".to_string() },
                PlaylistItem { id: "4".to_string(), name: "Playlist Example 4".to_string() },
            ],
            playlist_selected: 0,
            error_message: None,
            error_timestamp: None,
        }
    }
}

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
pub struct TrackMetadata {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            name: "No track playing".to_string(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
        }
    }
}

impl TrackMetadata {
    pub fn from_playback(playback: &CurrentPlaybackContext) -> Self {
        if let Some(item) = &playback.item {
            match item {
                PlayableItem::Track(track) => {
                    let artist = track
                        .artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_default();

                    Self {
                        name: track.name.clone(),
                        artist,
                        album: track.album.name.clone(),
                        duration_ms: track.duration.num_milliseconds() as u32,
                    }
                }
                PlayableItem::Episode(episode) => Self {
                    name: episode.name.clone(),
                    artist: episode.show.name.clone(),
                    album: "Podcast".to_string(),
                    duration_ms: episode.duration.num_milliseconds() as u32,
                },
                PlayableItem::Unknown(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }
}

#[derive(Clone)]
struct PlaybackTiming {
    position_ms: u32,
    last_update: Instant,
    is_playing: bool,
    duration_ms: u32,
}

impl Default for PlaybackTiming {
    fn default() -> Self {
        Self {
            position_ms: 0,
            last_update: Instant::now(),
            is_playing: false,
            duration_ms: 0,
        }
    }
}

impl PlaybackTiming {
    fn current_position_ms(&self) -> u32 {
        if self.is_playing && self.duration_ms > 0 {
            let elapsed = self.last_update.elapsed().as_millis() as u32;
            self.position_ms.saturating_add(elapsed).min(self.duration_ms)
        } else {
            self.position_ms.min(self.duration_ms.max(1) - 1)
        }
    }

    fn update_position(&mut self, new_position_ms: u32, is_playing: bool) {
        let current_calculated = self.current_position_ms();
        let diff = new_position_ms as i64 - current_calculated as i64;

        let state_changed = self.is_playing != is_playing;
        let significant_backward_jump = diff < -2000;
        let significant_forward_jump = diff > 2000;
        let was_paused = !self.is_playing;
        let acceptable_sync = diff >= -100;

        if state_changed || significant_backward_jump || significant_forward_jump || was_paused || acceptable_sync {
            self.position_ms = new_position_ms;
            self.last_update = Instant::now();
        }
        self.is_playing = is_playing;
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackSettings {
    pub device_name: String,
    pub shuffle: bool,
    pub repeat: RepeatState,
    pub volume: u8,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            device_name: "spotify-rs".to_string(),
            shuffle: false,
            repeat: RepeatState::Off,
            volume: 100,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackInfo {
    pub track: TrackMetadata,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub is_playing: bool,
    pub settings: PlaybackSettings,
}

impl Default for PlaybackInfo {
    fn default() -> Self {
        Self {
            track: TrackMetadata::default(),
            progress_ms: 0,
            duration_ms: 0,
            is_playing: false,
            settings: PlaybackSettings::default(),
        }
    }
}

pub struct AppModel {
    pub spotify: Option<SpotifyClient>,
    track_metadata: Arc<Mutex<TrackMetadata>>,
    playback_timing: Arc<Mutex<PlaybackTiming>>,
    playback_settings: Arc<Mutex<PlaybackSettings>>,
    pub ui_state: Arc<Mutex<UiState>>,
    pub should_quit: Arc<Mutex<bool>>,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            spotify: None,
            track_metadata: Arc::new(Mutex::new(TrackMetadata::default())),
            playback_timing: Arc::new(Mutex::new(PlaybackTiming::default())),
            playback_settings: Arc::new(Mutex::new(PlaybackSettings::default())),
            ui_state: Arc::new(Mutex::new(UiState::default())),
            should_quit: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_spotify_client(&mut self, client: SpotifyClient) {
        self.spotify = Some(client);
    }

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

    pub async fn auto_clear_old_errors(&self) {
        let mut state = self.ui_state.lock().await;
        if let Some(timestamp) = state.error_timestamp {
            if timestamp.elapsed().as_secs() > 5 {
                state.error_message = None;
                state.error_timestamp = None;
            }
        }
    }
}
