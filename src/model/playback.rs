//! Playback-related types and state management

use std::time::Instant;
use rspotify::model::{CurrentPlaybackContext, PlayableItem};
use rspotify::prelude::Id;

use crate::audio::DEFAULT_VOLUME_PERCENT;
use super::types::RepeatState;

/// Metadata about the currently playing track
#[derive(Clone, Debug)]
pub struct TrackMetadata {
    pub name: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u32,
    pub uri: String,
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self {
            name: "No track playing".to_string(),
            artist: String::new(),
            artists: Vec::new(),
            album: String::new(),
            duration_ms: 0,
            uri: String::new(),
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
                    
                    let all_artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();

                    let uri = track.id.as_ref()
                        .map(|id| format!("spotify:track:{}", id.id()))
                        .unwrap_or_default();

                    Self {
                        name: track.name.clone(),
                        artist,
                        artists: all_artists,
                        album: track.album.name.clone(),
                        duration_ms: track.duration.num_milliseconds() as u32,
                        uri,
                    }
                }
                PlayableItem::Episode(episode) => {
                    let uri = format!("spotify:episode:{}", episode.id.id());
                    Self {
                        name: episode.name.clone(),
                        artist: episode.show.name.clone(),
                        artists: vec![episode.show.name.clone()],
                        album: "Podcast".to_string(),
                        duration_ms: episode.duration.num_milliseconds() as u32,
                        uri,
                    }
                }
                PlayableItem::Unknown(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }
}

/// Internal timing state for smooth progress bar updates
#[derive(Clone)]
pub struct PlaybackTiming {
    pub position_ms: u32,
    pub last_update: Instant,
    pub is_playing: bool,
    pub duration_ms: u32,
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
    pub fn current_position_ms(&self) -> u32 {
        if self.is_playing && self.duration_ms > 0 {
            let elapsed = self.last_update.elapsed().as_millis() as u32;
            self.position_ms.saturating_add(elapsed).min(self.duration_ms)
        } else {
            self.position_ms.min(self.duration_ms.max(1) - 1)
        }
    }

    pub fn update_position(&mut self, new_position_ms: u32, is_playing: bool) {
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

/// Settings related to playback (device, shuffle, repeat, volume)
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
            volume: DEFAULT_VOLUME_PERCENT,
        }
    }
}

/// Complete playback information for rendering the UI
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
