//! Controller module - Application logic and event handling
//!
//! This module contains the application controller that handles user input,
//! coordinates between the model and view, and manages playback operations.
//! It is organized into submodules by responsibility:
//!
//! - `input`: Key event handling
//! - `playback`: Playback control methods
//! - `navigation`: Library/playlist/search navigation
//! - `player_events`: Librespot player event listener

mod input;
mod playback;
mod navigation;
mod player_events;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::audio::AudioBackend;
use crate::model::AppModel;

#[derive(Clone)]
pub struct AppController {
    pub(crate) model: Arc<Mutex<AppModel>>,
    pub(crate) audio_backend: Arc<Mutex<Option<AudioBackend>>>,
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
    pub(crate) async fn try_start_event_listener(&self) {
        let mut started = self.event_listener_started.lock().await;
        if *started {
            return;
        }

        let backend_guard = self.audio_backend.lock().await;
        if let Some(backend) = backend_guard.as_ref() {
            if let Some(event_channel) = backend.get_player_event_channel().await {
                *started = true;
                let audio_backend = self.audio_backend.clone();
                drop(backend_guard);
                drop(started);
                self.start_player_event_listener(event_channel, audio_backend);
            }
        }
    }

    pub(crate) fn format_error(error: &anyhow::Error) -> String {
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
            format!("Error: {}", error_str)
        }
    }
}
